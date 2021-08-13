#![feature(associated_type_bounds)]

// v0.1:
// - TODO word deletion
// - TODO word editing - make sure to edit *_full methods to reflect this
// - TODO user system
// - TODO attributions - editing users & references & so on
// - TODO remove changes_summary
// - TODO logging
// - TODO config

// v0.2:
// - suggestion publicising, voting & commenting
// - conjugation tables
// - user profiles showing statistics (for mods primarily but maybe can publicise it?)
// - backups
// - additional resources/links page
// - automated data-dump which can be downloaded
//        -> automate anki deck

// v0.3:
// - grammar notes
// - embedded blog (static site generator?) for transparency

// Stretch goals
// - forum for xhosa questions (discourse?)
// - donations for hosting costs (maybe even to pay native speakers to submit words?)

// Technical improvements:
// - TODO error handling - dont crash always probably & on panic, always crash (viz. tokio workers)!
// - TODO ratelimiting
// - TODO html/css/js min
// - TODO see if i can replace cloning pool with cloning conn?

use crate::session::{LiveSearchSession, WsMessage};
use crate::search::{WordHit, TantivyClient};
use accept::accept;
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

mod accept;
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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            database_path: PathBuf::from("isixhosa_click.db"),
            tantivy_path: PathBuf::from("tantivy_data/"),
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
    let tantivy = TantivyClient::start(&cfg.tantivy_path, pool.clone()).await.unwrap();

    task::spawn_blocking(move || {
        let conn = pool_clone.get().unwrap();
        conn.execute(include_str!("sql/words.sql"), params![])
            .unwrap();
        conn.execute(include_str!("sql/examples.sql"), params![])
            .unwrap();
        conn.execute(include_str!("sql/linked_words.sql"), params![])
            .unwrap();
        conn.execute(include_str!("sql/example_suggestions.sql"), params![])
            .unwrap();
        conn.execute(include_str!("sql/linked_word_suggestions.sql"), params![])
            .unwrap();
        conn.execute(include_str!("sql/word_suggestions.sql"), params![])
            .unwrap();
    })
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

    println!("Visit http://127.0.0.1:25565/submit");
    warp::serve(routes.with(warp::log("isixhosa")))
        .run(([0, 0, 0, 0], 25565))
        .await;
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
