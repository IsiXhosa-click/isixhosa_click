#![feature(associated_type_bounds)]

// Before launch:
// - TODO suggest possible duplicates in submit form
// - TODO better way to suggest plurals?

// Soon after launch, perhaps before:
// - standalone example & linked word suggestion editing
// - principled HTTP response codes with previous_success in form submit and so on
// - ability to search for and ban users
// - error handling - dont crash always probably & on panic, always crash (viz. tokio workers)!
// - move PartOfSpeech to isixhosa crate
// - better search engine optimisation
// - cache control headers/etags
// - integration testing
// - attributions - editing users & references & so on

// Well after launch:
// - forum for xhosa questions (discourse?)
// - rate limiting
// - tracing for logging over log: open telemetry/ELK stack or similar?
// - additional resources/links page
// - suggestion publicising, voting & commenting
// - conjugation tables
// - user profiles showing statistics (for mods primarily but maybe can publicise it?)
// - semantic fields/categories linking related words to browse all at once
// - grammar notes
// - embedded blog (static site generator?) for transparency

use crate::auth::{
    with_any_auth, Auth, DbBase, Permissions, PublicAccessDb, Unauthorized, UnauthorizedReason,
};
use crate::database::existing::ExistingWord;
use crate::format::DisplayHtml;
use crate::search::{IncludeResults, TantivyClient, WordHit};
use crate::session::{LiveSearchSession, WsMessage};
use askama::Template;
use auth::auth;
use chrono::Local;
use details::details;
use edit::edit;
use futures::StreamExt;
use log::LevelFilter;
use log4rs::append::console::ConsoleAppender;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config as LogConfig, Root};
use log4rs::encode::pattern::PatternEncoder;
use moderation::accept;
use percent_encoding::NON_ALPHANUMERIC;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::fmt::Debug;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use submit::submit;
use tokio::task;
use warp::filters::compression::gzip;
use warp::http::header::CONTENT_TYPE;
use warp::http::uri::Authority;
use warp::http::{uri, StatusCode, Uri};
use warp::path::FullPath;
use warp::reject::Reject;
use warp::reply::Response;
use warp::{path, Filter, Rejection, Reply};
use xtra::spawn::TokioGlobalSpawnExt;
use xtra::Actor;

mod auth;
mod database;
mod details;
mod edit;
mod export;
mod format;
mod language;
mod moderation;
mod search;
mod serialization;
mod session;
mod submit;

#[derive(Debug)]
struct TemplateError(askama::Error);

impl Reject for TemplateError {}

#[derive(Serialize, Deserialize)]
pub struct Config {
    database_path: PathBuf,
    tantivy_path: PathBuf,
    cert_path: PathBuf,
    key_path: PathBuf,
    static_site_files: PathBuf,
    other_static_files: PathBuf,
    log_path: PathBuf,
    log_level: LevelFilter,
    http_port: u16,
    https_port: u16,
    host: String,
    oidc_client: String,
    oidc_secret: String,
    plaintext_export_path: PathBuf,
}

impl Config {
    pub fn host_builder(host: &str, port: u16) -> uri::Builder {
        let authority = Authority::from_str(&format!("{}:{}", host, port)).unwrap();
        Uri::builder().scheme("https").authority(authority)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            database_path: PathBuf::from("isixhosa_click.db"),
            tantivy_path: PathBuf::from("tantivy_data/"),
            cert_path: PathBuf::from("tls/cert.pem"),
            key_path: PathBuf::from("tls/key.rsa"),
            static_site_files: PathBuf::from("static/"),
            other_static_files: PathBuf::from("dummy_www/"),
            log_path: PathBuf::from("log/"),
            log_level: LevelFilter::Info,
            http_port: 8080,
            https_port: 8443,
            host: "127.0.0.01".to_string(),
            oidc_client: "".to_string(),
            oidc_secret: "".to_string(),
            plaintext_export_path: PathBuf::from("isixhosa_click_export/"),
        }
    }
}

fn init_logging(cfg: &Config) {
    const LOG_PATTERN: &str = "[{d(%Y-%m-%d %H:%M:%S)} {h({l})} {M}] {m}{n}";

    let path = cfg
        .log_path
        .join(Local::now().to_rfc3339())
        .with_extension("log");

    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(LOG_PATTERN)))
        .build();

    let file = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(LOG_PATTERN)))
        .build(path)
        .unwrap();

    let root = Root::builder()
        .appender("stdout")
        .appender("file")
        .build(cfg.log_level);

    let log_config = LogConfig::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .appender(Appender::builder().build("file", Box::new(file)))
        .build(root)
        .unwrap();

    log4rs::init_config(log_config).unwrap();
}

async fn process_body<F, E>(response: Response, minify: F) -> Result<Response, Rejection>
where
    F: FnOnce(&str) -> Result<String, E>,
    E: Debug,
{
    let (parts, body) = response.into_parts();
    let bytes = warp::hyper::body::to_bytes(body).await.unwrap();
    let body_str = std::str::from_utf8(&bytes).unwrap();
    let minified = minify(body_str).unwrap();
    Ok(Response::from_parts(parts, minified.into()))
}

async fn minify<R: Reply>(reply: R) -> Result<impl Reply, Rejection> {
    let response = reply.into_response();
    if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
        let content_type = content_type.to_str().unwrap();

        if content_type.starts_with("text/html") {
            #[allow(clippy::redundant_closure)] // lifetime issue
            return process_body(response, |s| html_minifier::minify(s)).await;
        } else if content_type.starts_with("text/css") {
            return process_body(response, minifier::css::minify).await;
        }
    }

    Ok(response)
}

async fn handle_auth_error(err: Rejection) -> Result<Response, Rejection> {
    if let Some(unauthorized) = err.find::<Unauthorized>() {
        let redirect_to = |to| {
            warp::http::Response::builder()
                .status(StatusCode::FOUND)
                .header(warp::http::header::LOCATION, to)
                .body("")
                .unwrap()
                .into_response()
        };

        match unauthorized.reason {
            UnauthorizedReason::NotLoggedIn => {
                let login = format!(
                    "/login/oauth2/authorization/oidc?redirect={}",
                    percent_encoding::utf8_percent_encode(
                        unauthorized.redirect.as_str(),
                        NON_ALPHANUMERIC
                    ),
                );

                Ok(redirect_to(login))
            }
            UnauthorizedReason::NoPermissions | UnauthorizedReason::Locked => {
                // TODO(ban)
                Ok(warp::reply::with_status(warp::reply(), StatusCode::FORBIDDEN).into_response())
            }
            UnauthorizedReason::InvalidCookie => {
                Ok(redirect_to("/login/oauth2/authorization/oidc".to_owned()))
            }
        }
    } else {
        Err(err)
    }
}

fn main() {
    let cfg: Config = confy::load("isixhosa_click").unwrap();

    let flag = std::env::args().nth(1);
    let flag = flag.as_ref();

    if flag.map(|s| s == "--run-backup").unwrap_or(false) {
        export::run_daily_tasks(cfg);
    } else if flag.map(|s| s == "--restore-from-backup").unwrap_or(false) {
        export::restore(cfg);
    } else if let Some(flag) = flag {
        eprintln!(
            "Unknown flag: {}. Accepted values are --run-backup and --restore-from-backup.",
            flag
        );
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(server(cfg));
    }
}

fn set_up_db(conn: &Connection) {
    const CREATIONS: [&str; 12] = [
        include_str!("sql/users.sql"),
        include_str!("sql/words.sql"),
        include_str!("sql/user_attributions.sql"),
        include_str!("sql/word_suggestions.sql"),
        include_str!("sql/word_deletion_suggestions.sql"),
        include_str!("sql/examples.sql"),
        include_str!("sql/example_suggestions.sql"),
        include_str!("sql/example_deletion_suggestions.sql"),
        include_str!("sql/linked_words.sql"),
        include_str!("sql/linked_word_suggestions.sql"),
        include_str!("sql/linked_word_deletion_suggestions.sql"),
        include_str!("sql/login_tokens.sql"),
    ];

    // See https://github.com/the-lean-crate/criner/discussions/5
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA wal_autocheckpoint = 1000;
        PRAGMA wal_checkpoint(TRUNCATE);
    ",
    )
    .unwrap();

    for creation in &CREATIONS {
        conn.execute(creation, params![]).unwrap();
    }
}

async fn server(cfg: Config) {
    init_logging(&cfg);
    log::info!("IsiXhosa server startup");

    let manager = SqliteConnectionManager::file(&cfg.database_path);
    let pool = Pool::new(manager).unwrap();
    let pool_clone = pool.clone();
    task::spawn_blocking(move || set_up_db(&pool_clone.get().unwrap()))
        .await
        .unwrap();

    let tantivy = TantivyClient::start(&cfg.tantivy_path, pool.clone())
        .await
        .unwrap();

    let tantivy_cloned = tantivy.clone();
    let with_tantivy = warp::any().map(move || tantivy_cloned.clone());
    let db = DbBase::new(pool);

    let search_page = with_any_auth(db.clone()).map(|auth, _db| Search {
        auth,
        hits: Default::default(),
        query: Default::default(),
    });
    let query_search = warp::query()
        .and(with_any_auth(db.clone()))
        .and(with_tantivy.clone())
        .and_then(query_search);
    let live_search = with_any_auth(db.clone())
        .and(warp::ws())
        .and(with_tantivy)
        .and(warp::query())
        .map(live_search);
    let search = warp::path("search")
        .and(path::end())
        .and(live_search.or(query_search).or(search_page));
    let terms_of_use = warp::path("terms_of_use")
        .and(path::end())
        .and(with_any_auth(db.clone()))
        .map(|auth, _| TermsOfUsePage { auth });

    let about = warp::get()
        .and(warp::path("about"))
        .and(path::end())
        .and(with_any_auth(db.clone()))
        .and_then(|auth, db| async move {
            Ok::<AboutPage, Infallible>(AboutPage {
                auth,
                word_count: tokio::task::spawn_blocking(move || ExistingWord::count_all(&db))
                    .await
                    .unwrap(),
            })
        });

    let routes = warp::fs::dir(cfg.static_site_files.clone())
        .or(warp::fs::dir(cfg.other_static_files.clone()))
        .or(search)
        .or(terms_of_use)
        .or(submit(db.clone(), tantivy.clone()))
        .or(accept(db.clone(), tantivy.clone()))
        .or(details(db.clone()))
        .or(edit(db.clone(), tantivy))
        .or(warp::get()
            .and(path::end())
            .map(|| warp::redirect(Uri::from_static("/search"))))
        .or(about)
        .or(auth(db.clone(), &cfg).await)
        .recover(handle_auth_error)
        .or(auth::with_any_auth(db).map(|auth, _db| {
            warp::reply::with_status(NotFound { auth }, StatusCode::NOT_FOUND).into_response()
        }))
        .boxed();

    log::info!("Visit https://127.0.0.1:{}/", cfg.https_port);

    let redirect = warp::path::full().map(move |path: FullPath| {
        let to = Uri::builder()
            .scheme("https")
            .authority("isixhosa.click")
            .path_and_query(path.as_str())
            .build()
            .unwrap();
        warp::redirect(to)
    });
    let http_redirect = warp::serve(redirect);

    tokio::spawn(http_redirect.run(([0, 0, 0, 0], cfg.http_port)));

    // Add post filters such as minification, logging, and gzip
    let serve = routes
        .and_then(minify)
        .with(warp::log("isixhosa"))
        .with(gzip());

    warp::serve(serve)
        .tls()
        .cert_path(cfg.cert_path)
        .key_path(cfg.key_path)
        .run(([0, 0, 0, 0], cfg.https_port))
        .await
}

#[derive(Deserialize, Clone, Debug)]
struct LiveSearchParams {
    include_own_suggestions: Option<bool>,
}

#[derive(Deserialize, Clone, Debug)]
struct SearchQuery {
    query: String,
}

#[derive(Template, Clone, Debug)]
#[template(path = "404.askama.html")]
struct NotFound {
    auth: Auth,
}

#[derive(Template, Clone, Debug)]
#[template(path = "about.askama.html")]
struct AboutPage {
    auth: Auth,
    word_count: u64,
}

#[derive(Template, Clone, Debug)]
#[template(path = "terms_of_use.askama.html")]
struct TermsOfUsePage {
    auth: Auth,
}

#[derive(Template)]
#[template(path = "search.askama.html")]
struct Search {
    auth: Auth,
    hits: Vec<WordHit>,
    query: String,
}

async fn query_search(
    query: SearchQuery,
    auth: Auth,
    _db: impl PublicAccessDb,
    tantivy: Arc<TantivyClient>,
) -> Result<impl warp::Reply, Rejection> {
    let results = tantivy
        .search(query.query.clone(), IncludeResults::AcceptedOnly)
        .await
        .unwrap();

    Ok(Search {
        auth,
        query: query.query,
        hits: results,
    })
}

fn live_search(
    auth: Auth,
    _db: impl PublicAccessDb,
    ws: warp::ws::Ws,
    tantivy: Arc<TantivyClient>,
    params: LiveSearchParams,
) -> impl warp::Reply {
    ws.on_upgrade(move |websocket| {
        let (sender, stream) = websocket.split();
        let include_suggestions_from_user = if params.include_own_suggestions.unwrap_or(false) {
            auth.user_id()
        } else {
            None
        };

        let addr = LiveSearchSession::new(
            sender,
            tantivy,
            include_suggestions_from_user,
            auth.has_permissions(Permissions::Moderator),
        )
        .create(Some(4))
        .spawn_global();
        tokio::spawn(addr.attach_stream(stream.map(WsMessage)));
        futures::future::ready(())
    })
}
