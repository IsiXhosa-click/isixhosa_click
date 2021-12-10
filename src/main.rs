// Soon after launch, perhaps before:
// - informal/archaic meanings
// - standalone example & linked word suggestion editing
// - forum for xhosa questions
// - error handling - dont crash always probably & on panic, always crash (viz. tokio workers)!
// - better search engine optimisation
// - cache control headers/etags
// - attributions - references
// - learn page with additional resources/links page

// Well after launch:
// - rate limiting
// - integration testing
// - tracing for logging over log: open telemetry/ELK stack or similar?
// - conjugation tables
// - user profiles showing statistics (for mods primarily but maybe can publicise it?)
// - semantic fields/categories linking related words to browse all at once
// - grammar notes
// - embedded blog (static site generator?) for transparency

use std::collections::HashSet;
use crate::auth::*;
use crate::database::existing::ExistingWord;
use crate::database::suggestion::SuggestedWord;
use crate::format::DisplayHtml;
use crate::search::{IncludeResults, TantivyClient, WordHit};
use crate::serialization::false_fn;
use crate::session::{LiveSearchSession, WsMessage};
use askama::Template;
use auth::auth;
use details::details;
use edit::edit;
use futures::StreamExt;
use moderation::moderation;
use opentelemetry::global;
use opentelemetry::sdk::propagation::TraceContextPropagator;
use percent_encoding::NON_ALPHANUMERIC;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::fmt::Debug;
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use submit::submit;
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, Span};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, Registry};
use warp::filters::compression::gzip;
use warp::filters::BoxedFilter;
use warp::http::header::CONTENT_TYPE;
use warp::http::uri::Authority;
use warp::http::{uri, StatusCode, Uri};
use warp::path::FullPath;
use warp::reject::{MethodNotAllowed, Reject};
use warp::reply::Response;
use warp::{path, reply, Filter, Rejection, Reply};
use warp_reverse_proxy as proxy;
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

pub fn spawn_blocking_child<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let span = Span::current();
    tokio::task::spawn_blocking(move || {
        let _g = span.enter();
        f()
    })
}

pub trait DebugBoxedExt: Filter {
    #[cfg(debug_assertions)]
    fn debug_boxed(self) -> BoxedFilter<Self::Extract>;

    #[cfg(not(debug_assertions))]
    fn debug_boxed(self) -> Self;
}

impl<F> DebugBoxedExt for F
where
    F: Filter + Send + Sync + 'static,
    F::Extract: Send,
    F::Error: Into<Rejection>,
{
    #[cfg(debug_assertions)]
    fn debug_boxed(self) -> BoxedFilter<Self::Extract> {
        self.boxed()
    }

    #[cfg(not(debug_assertions))]
    fn debug_boxed(self) -> Self {
        self
    }
}

pub trait DebugExt {
    fn to_debug(&self) -> String;
}

impl<T: Debug> DebugExt for T {
    fn to_debug(&self) -> String {
        format!("{:?}", self)
    }
}

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
            http_port: 8080,
            https_port: 8443,
            host: "127.0.0.01".to_string(),
            oidc_client: "".to_string(),
            oidc_secret: "".to_string(),
            plaintext_export_path: PathBuf::from("isixhosa_click_export/"),
        }
    }
}

fn init_tracing() {
    global::set_text_map_propagator(TraceContextPropagator::new());
    let tracer = opentelemetry_jaeger::new_pipeline()
        .with_service_name("isixhosa.click")
        .install_batch(opentelemetry::runtime::Tokio)
        .unwrap();

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let fmt_layer = tracing_subscriber::fmt::layer().with_target(false);

    Registry::default()
        .with(LevelFilter::DEBUG)
        .with(telemetry)
        .with(fmt_layer)
        .init();
}

#[instrument(
    name = "Minify outgoing data",
    fields(unminified, minified, saving),
    skip_all
)]
async fn process_body<F, E>(response: Response, minify: F) -> Result<Response, Rejection>
where
    F: FnOnce(&str) -> Result<String, E>,
    E: Debug,
{
    let span = Span::current();
    let (parts, body) = response.into_parts();
    let bytes = warp::hyper::body::to_bytes(body).await.unwrap();
    let unminified = std::str::from_utf8(&bytes).unwrap();
    let minified = minify(unminified).unwrap();

    span.record("unminified", &unminified.len());
    span.record("minified", &minified.len());

    let saving = if !unminified.is_empty() {
        (1.0 - (minified.len() as f64 / unminified.len() as f64)) * 100.0
    } else {
        0.0
    };

    span.record("saving", &format!("{:.2}%", saving).as_str());

    Ok(Response::from_parts(parts, minified.into()))
}

async fn minify<R: Reply>(reply: R) -> Result<impl Reply, Rejection> {
    let response = reply.into_response();
    if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
        let mime = content_type.to_str().unwrap();

        if mime.starts_with("text/html") {
            #[allow(clippy::redundant_closure)] // lifetime issue
            return process_body(response, |s| html_minifier::minify(s)).await;
        } else if mime.starts_with("text/javascript") || mime.starts_with("application/javascript")
        {
            return process_body(response, |s| Ok::<String, ()>(minifier::js::minify(s))).await;
        } else if mime.starts_with("text/css") {
            return process_body(response, minifier::css::minify).await;
        }
    }

    Ok(response)
}

#[instrument(name = "Handle errors")]
async fn handle_error(err: Rejection) -> Result<Response, Rejection> {
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
                debug!("User was not logged in; redirecting");
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
                debug!("User has insufficient permissions");
                Ok(warp::reply::with_status(warp::reply(), StatusCode::FORBIDDEN).into_response())
            }
            UnauthorizedReason::InvalidCookie => {
                debug!("User has invalid cookie; redirecting");
                Ok(redirect_to("/login/oauth2/authorization/oidc".to_owned()))
            }
        }
    } else if err.find::<MethodNotAllowed>().is_some() {
        Err(warp::reject::not_found())
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

#[instrument("Set up database PRAGMAs and tables", skip_all)]
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

// I cannot be bothered trying to find the right type
macro_rules! wrap_filter {
    ($f:expr) => {
        $f
            .and_then(minify)
            .with(warp::trace(|info| {
                tracing::info_span!(
                    "HTTPS request",
                    method = %info.method(),
                    path = %info.path(),
                )
            }))
            .with(gzip())
    }
}

async fn server(cfg: Config) {
    init_tracing();
    info!("IsiXhosa server startup");

    let manager = SqliteConnectionManager::file(&cfg.database_path);
    let pool = Pool::new(manager).unwrap();
    let pool_clone = pool.clone();
    spawn_blocking_child(move || set_up_db(&pool_clone.get().unwrap()))
        .await
        .unwrap();

    let tantivy = TantivyClient::start(&cfg.tantivy_path, pool.clone())
        .await
        .unwrap();

    let tantivy_cloned = tantivy.clone();
    let with_tantivy = warp::any().map(move || tantivy_cloned.clone());
    let db = DbBase::new(pool);

    let search = {
        let search_page = with_any_auth(db.clone()).map(|auth, _db| Search {
            auth,
            hits: Default::default(),
            query: Default::default(),
        });

        let query_search = warp::path::end()
            .and(warp::query())
            .and(with_tantivy.clone())
            .and(with_any_auth(db.clone()))
            .and_then(query_search);
        let live_search = warp::path::end()
            .and(warp::ws())
            .and(with_tantivy.clone())
            .and(warp::query())
            .and(with_any_auth(db.clone()))
            .map(live_search);
        let duplicate_search = warp::path("duplicates")
            .and(warp::path::end())
            .and(warp::query())
            .and(with_tantivy)
            .and(with_moderator_auth(db.clone()))
            .and_then(duplicate_search);

        warp::path("search")
            .and(
                duplicate_search
                    .or(live_search)
                    .or(query_search)
                    .or(search_page),
            )
            .debug_boxed()
    };

    let simple_templates = {
        let terms_of_use = warp::path("terms_of_use")
            .and(path::end())
            .and(with_any_auth(db.clone()))
            .map(|auth, _| TermsOfUse { auth });
        let style_guide = warp::path("style_guide")
            .and(path::end())
            .and(with_any_auth(db.clone()))
            .map(|auth, _| StyleGuide { auth });

        let about = warp::get()
            .and(warp::path("about"))
            .and(path::end())
            .and(with_any_auth(db.clone()))
            .and_then(|auth, db| async move {
                Ok::<About, Infallible>(About {
                    auth,
                    word_count: spawn_blocking_child(move || ExistingWord::count_all(&db))
                        .await
                        .unwrap(),
                })
            });

        terms_of_use.or(about).or(style_guide).debug_boxed()
    };

    let redirects = {
        let favico_redirect = warp::get()
            .and(warp::path("favicon.ico"))
            .map(|| warp::redirect(Uri::from_static("/icons/favicon.ico")));

        let index_redirect = warp::get()
            .and(path::end())
            .map(|| warp::redirect(Uri::from_static("/search")));

        favico_redirect.or(index_redirect).debug_boxed()
    };

    let jaeger_proxy = {
        let base = warp::path!("admin" / "jaeger" / ..);
        let forward_url = "http://127.0.0.1:16686".to_owned();
        let forward =
            proxy::reverse_proxy_filter(String::new(), forward_url).with(warp::trace(|_info| {
                tracing::info_span!("Forward jaeger request")
            }));
        let proxy = with_administrator_auth(db.clone())
            .and(forward)
            .recover(handle_error)
            .with(warp::trace(|info| {
                tracing::info_span!(
                    "Jaeger reverse proxy request",
                    method = %info.method(),
                    path = %info.path(),
                )
            }));

        base.and(proxy).debug_boxed()
    };

    let static_files = warp::fs::dir(cfg.static_site_files.clone())
        .or(warp::fs::dir(cfg.other_static_files.clone()));

    let routes = search
        .or(simple_templates)
        .or(redirects)
        .or(submit(db.clone(), tantivy.clone()))
        .or(moderation(db.clone(), tantivy.clone()))
        .or(details(db.clone()))
        .or(edit(db.clone(), tantivy))
        .or(auth(db.clone(), &cfg).await)
        .or(static_files)
        .recover(handle_error)
        .debug_boxed();

    info!("Visit https://127.0.0.1:{}/", cfg.https_port);

    let http_redirect = warp::path::full()
        .map(move |path: FullPath| {
            let to = Uri::builder()
                .scheme("https")
                .authority("isixhosa.click")
                .path_and_query(path.as_str())
                .build()
                .unwrap();
            warp::redirect(to)
        })
        .with(warp::trace(|info| {
            tracing::info_span!(
                    "HTTP redirect",
                    method = %info.method(),
                    path = %info.path(),
            )
        }));

    let http_redirect = warp::serve(http_redirect);

    tokio::spawn(http_redirect.run(([0, 0, 0, 0], cfg.http_port)));

    // Add post filters such as minification, logging, and gzip
    let serve = jaeger_proxy
        .or(wrap_filter!(routes))
        .or(wrap_filter!(auth::with_any_auth(db).map(|auth, _db| {
            warp::reply::with_status(NotFound { auth }, StatusCode::NOT_FOUND).into_response()
        })));

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
    #[serde(alias = "q")]
    query: String,
    #[serde(default = "false_fn")]
    raw: bool,
}

#[derive(Template, Clone, Debug)]
#[template(path = "404.askama.html")]
struct NotFound {
    auth: Auth,
}

#[derive(Template, Clone, Debug)]
#[template(path = "about.askama.html")]
struct About {
    auth: Auth,
    word_count: u64,
}

#[derive(Template, Clone, Debug)]
#[template(path = "terms_of_use.askama.html")]
struct TermsOfUse {
    auth: Auth,
}

#[derive(Template, Clone, Debug)]
#[template(path = "style_guide.askama.html")]
struct StyleGuide {
    auth: Auth,
}

#[derive(Template)]
#[template(path = "search.askama.html")]
struct Search {
    auth: Auth,
    hits: Vec<WordHit>,
    query: String,
}

#[instrument(
    name = "Search with a query string",
    fields(
        query = %query.query,
        raw = %query.raw,
    ),
    skip_all,
)]
async fn query_search(
    query: SearchQuery,
    tantivy: Arc<TantivyClient>,
    auth: Auth,
    _db: impl PublicAccessDb,
) -> Result<impl warp::Reply, Rejection> {
    let results = tantivy
        .search(query.query.clone(), IncludeResults::AcceptedOnly, false)
        .await
        .unwrap();

    if !query.raw {
        let template = Search {
            auth,
            query: query.query,
            hits: results,
        };

        Ok(askama_warp::reply(&template, "html"))
    } else {
        Ok(reply::json(&results).into_response())
    }
}

#[derive(Deserialize, Debug)]
struct DuplicateQuery {
    suggestion: NonZeroU64,
}

#[instrument(
    name = "Search for duplicates of a suggestion",
    fields(suggestion_id = %query.suggestion),
    skip_all,
)]
async fn duplicate_search(
    query: DuplicateQuery,
    tantivy: Arc<TantivyClient>,
    _user: User,
    db: impl ModeratorAccessDb,
) -> Result<impl warp::Reply, Rejection> {
    let suggestion = SuggestedWord::fetch_alone(&db, query.suggestion.get());

    let include = IncludeResults::AcceptedAndAllSuggestions;
    let res = match suggestion.filter(|w| w.word_id.is_none()) {
        Some(w) => {
            let english = tantivy
                .search(w.english.current().clone(), include, true)
                .await
                .unwrap();
            let xhosa = tantivy
                .search(w.xhosa.current().clone(), include, true)
                .await
                .unwrap();

            let mut results = HashSet::with_capacity(english.len() + xhosa.len());
            results.extend(english);
            results.extend(xhosa);
            results.retain(|res| !(res.id == query.suggestion.get() && res.is_suggestion));
            results
        }
        None => HashSet::new(),
    };

    Ok(reply::json(&res))
}

#[instrument(
    name = "Begin live search websocket connection",
    fields(include_own_suggestions = %params.include_own_suggestions.unwrap_or_default()),
    skip_all,
)]
fn live_search(
    ws: warp::ws::Ws,
    tantivy: Arc<TantivyClient>,
    params: LiveSearchParams,
    auth: Auth,
    _db: impl PublicAccessDb,
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
