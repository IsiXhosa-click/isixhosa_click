#![feature(associated_type_bounds)]

// v0.1:
// - TODO word, example, and linked word deletion
// - TODO word, example, and linked word editing - make sure to edit *_full methods to reflect this
// - TODO user system
// - TODO attributions - editing users & references & so on
// - TODO logging
// - TODO config for static directories
// - TODO redirect rest of URL
// - TODO opengraph image embed

// v0.2:
// - set up certbot
// - ratelimiting
// - error handling - dont crash always probably & on panic, always crash (viz. tokio workers)!
// - weekly drive backups
// - automated data-dump & backups of the database content which can be downloaded

// v0.3
// - additional resources/links page
// - suggestion publicising, voting & commenting
// - conjugation tables
// - user profiles showing statistics (for mods primarily but maybe can publicise it?)
// - automate anki deck creation

// v0.4:
// - semantic fields/categories linking related words to browse all at once
// - grammar notes
// - embedded blog (static site generator?) for transparency

// Stretch goals
// - forum for xhosa questions (discourse?)
// - donations for hosting costs (maybe even to pay native speakers to submit words?)

// Ideas:
// - html/css/js min
// - see if i can replace cloning pool with cloning conn?

use crate::session::{LiveSearchSession, WsMessage};
use crate::search::{WordHit, TantivyClient};
use moderation::accept;
use askama::Template;
use details::details;
use edit::edit;
use futures::StreamExt;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use submit::submit;
use tokio::task;
use warp::http::Uri;
use warp::reject::Reject;
use warp::{path, Filter, Rejection};
use xtra::spawn::TokioGlobalSpawnExt;
use xtra::Actor;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::path::PathBuf;
use std::convert::TryFrom;

mod moderation;
// mod auth;
mod database;
mod details;
mod edit;
mod language;
mod session;
mod submit;
mod search;

#[derive(Debug)]
struct TemplateError(askama::Error);

impl Reject for TemplateError {}

#[derive(Serialize, Deserialize)]
pub struct Config {
    database_path: PathBuf,
    tantivy_path: PathBuf,
    cert_path: PathBuf,
    key_path: PathBuf,
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
            http_port: 8080,
            https_port: 8443,
        }
    }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let cfg: Config = confy::load("isixhosa_click").unwrap();
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

    let tantivy = TantivyClient::start(&cfg.tantivy_path, pool.clone()).await.unwrap();

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

    let routes = warp::fs::dir("static")
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

    println!("Visit https://127.0.0.1:{}/", cfg.https_port);

    let redirect_uri = Uri::try_from("https://isixhosa.click").unwrap();
    let http_redirect = warp::serve(warp::any().map(move || warp::redirect(redirect_uri.clone())));

    tokio::spawn(http_redirect.run(([0, 0, 0, 0], cfg.http_port)));

    warp::serve(routes.with(warp::log("isixhosa")))
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

#[derive(Template)]
#[template(path = "404.html")]
struct NotFound;

#[derive(Template)]
#[template(path = "about.html")]
struct AboutPage;

#[derive(Template, Default)]
#[template(path = "search.html")]
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
