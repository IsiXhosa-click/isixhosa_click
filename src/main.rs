#![feature(associated_type_bounds)]

// Before launch:
// - TODO linked word deletion and editing
// - TODO user system

// Soon after launch, perhaps before:
// - error handling - dont crash always probably & on panic, always crash (viz. tokio workers)!
// - weekly drive backups
// - automated data-dump & backups of the database content which can be downloaded
// - move PartOfSpeech to isixhosa crate
// - attributions - editing users & references & so on
// - better search engine optimisation
// - cache control headers/etags
// - gzip compress

// Well after launch:
// - ratelimiting
// - additional resources/links page
// - suggestion publicising, voting & commenting
// - conjugation tables
// - user profiles showing statistics (for mods primarily but maybe can publicise it?)
// - automate anki deck creation
// - semantic fields/categories linking related words to browse all at once
// - grammar notes
// - embedded blog (static site generator?) for transparency

// Stretch goals
// - forum for xhosa questions (discourse?)
// - donations for hosting costs (maybe even to pay native speakers to submit words?)

// Ideas:
// - see if i can replace cloning pool with cloning conn?

use crate::language::NounClassExt;
use crate::search::{TantivyClient, WordHit};
use crate::serialization::OptionMapNounClassExt;
use crate::session::{LiveSearchSession, WsMessage};
use askama::Template;
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
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use submit::submit;
use tokio::task;
use warp::filters::compression::gzip;
use warp::http::header::CONTENT_TYPE;
use warp::http::Uri;
use warp::path::FullPath;
use warp::reject::Reject;
use warp::reply::Response;
use warp::{path, Filter, Rejection, Reply};
use xtra::spawn::TokioGlobalSpawnExt;
use xtra::Actor;

mod moderation;
// mod auth;
mod database;
mod details;
mod edit;
mod language;
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
    struct TimeItGuard(Instant);

    impl Drop for TimeItGuard {
        fn drop(&mut self) {
            log::debug!("Minify took {}us to complete", self.0.elapsed().as_micros());
        }
    }

    let _g = TimeItGuard(Instant::now());

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

#[tokio::main]
async fn main() {
    let cfg: Config = confy::load("isixhosa_click").unwrap();
    init_logging(&cfg);
    log::info!("IsiXhosa server startup");

    let manager = SqliteConnectionManager::file(&cfg.database_path);
    let pool = Pool::new(manager).unwrap();
    let pool_clone = pool.clone();

    task::spawn_blocking(move || {
        const CREATIONS: [&str; 9] = [
            include_str!("sql/words.sql"),
            include_str!("sql/word_suggestions.sql"),
            include_str!("sql/word_deletion_suggestions.sql"),
            include_str!("sql/examples.sql"),
            include_str!("sql/example_suggestions.sql"),
            include_str!("sql/example_deletion_suggestions.sql"),
            include_str!("sql/linked_words.sql"),
            include_str!("sql/linked_word_suggestions.sql"),
            include_str!("sql/linked_word_deletion_suggestions.sql"),
        ];

        let conn = pool_clone.get().unwrap();

        for creation in &CREATIONS {
            conn.execute(creation, params![]).unwrap();
        }
    })
    .await
    .unwrap();

    let tantivy = TantivyClient::start(&cfg.tantivy_path, pool.clone())
        .await
        .unwrap();

    let tantivy_cloned = tantivy.clone();
    let tantivy_filter = warp::any().map(move || tantivy_cloned.clone());

    let search_page = warp::any().map(Search::default);
    let query_search = warp::query()
        .and(tantivy_filter.clone())
        .and_then(query_search);
    let live_search = warp::ws().and(tantivy_filter).map(live_search);
    let search = warp::path("search")
        .and(path::end())
        .and(live_search.or(query_search).or(search_page));
    let about = warp::get()
        .and(warp::path("about"))
        .and(path::end())
        .map(|| AboutPage);

    let routes = warp::fs::dir(cfg.static_site_files)
        .or(warp::fs::dir(cfg.other_static_files))
        .or(search)
        .or(submit(pool.clone()))
        .or(accept(pool.clone(), tantivy))
        .or(details(pool.clone()))
        .or(edit(pool))
        .or(warp::get()
            .and(path::end())
            .map(|| warp::redirect(Uri::from_static("/search"))))
        .or(about)
        .or(warp::any().map(|| NotFound));

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
struct SearchQuery {
    query: String,
}

#[derive(Template, Clone, Copy, Debug)]
#[template(path = "404.askama.html")]
struct NotFound;

#[derive(Template, Clone, Copy, Debug)]
#[template(path = "about.askama.html")]
struct AboutPage;

#[derive(Template, Default)]
#[template(path = "search.askama.html")]
struct Search {
    hits: Vec<WordHit>,
    query: String,
}

async fn query_search(
    query: SearchQuery,
    tantivy: Arc<TantivyClient>,
) -> Result<impl warp::Reply, Rejection> {
    let results = tantivy.search(query.query.clone()).await.unwrap();

    Ok(Search {
        query: query.query,
        hits: results,
    })
}

fn live_search(ws: warp::ws::Ws, tantivy: Arc<TantivyClient>) -> impl warp::Reply {
    ws.on_upgrade(move |websocket| {
        let (sender, stream) = websocket.split();
        let addr = LiveSearchSession::new(sender, tantivy)
            .create(Some(4))
            .spawn_global();
        tokio::spawn(addr.attach_stream(stream.map(WsMessage)));
        futures::future::ready(())
    })
}
