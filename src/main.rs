#![feature(associated_type_bounds)]

// v0.1:
// - TODO word deletion
// - TODO word editing - make sure to edit *_full methods to reflect this
// - TODO suggestion publicising, voting & commenting
// - TODO user system
// - TODO attributions - editing users & references & so on

// - TODO "are you sure" dialogues for /accept
// - TODO basic styling
// - TODO about page

// v0.2:
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
use crate::typesense::{TypesenseClient, ShortWordSearchHit};
use accept::accept;
use arcstr::ArcStr;
use askama::Template;
use futures::StreamExt;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::Deserialize;
use submit::submit;
use details::details;
use tokio::task;
use warp::reject::Reject;
use warp::{path, Filter, Rejection};
use xtra::spawn::TokioGlobalSpawnExt;
use xtra::Actor;

mod accept;
// mod auth;
mod database;
mod language;
mod session;
mod submit;
mod typesense;
mod details;

#[derive(Debug)]
struct TemplateError(askama::Error);

impl Reject for TemplateError {}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let manager = SqliteConnectionManager::file("isixhosa_xyz.db");
    let pool = Pool::new(manager).unwrap();
    let pool_clone = pool.clone();
    let typesense = TypesenseClient {
        api_key: ArcStr::from(std::env::var("TYPESENSE_API_KEY").unwrap()),
    };

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

    let collection_created = typesense.create_collection_if_not_exists().await.unwrap();

    if collection_created {
        typesense.reindex_database(pool.clone()).await;
        eprintln!("Database reindexed.");
    }

    let typesense_cloned = typesense.clone();
    let typesense_filter = warp::any().map(move || typesense_cloned.clone());

    let search_page = warp::any().map(Search::default);
    let query_search = warp::query()
        .and(typesense_filter.clone())
        .and_then(query_search);
    let live_search = warp::ws().and(typesense_filter).map(live_search);
    let search = warp::path("search")
        .and(path::end())
        .and(live_search.or(query_search).or(search_page));

    let routes = warp::fs::dir("static")
        .or(search)
        .or(submit(pool.clone()))
        .or(accept(pool.clone(), typesense))
        .or(details(pool))
        .or(warp::get().and(path::end()).map(|| MainPage))
        .or(warp::any().map(|| NotFound));

    println!("Visit http://127.0.0.1:8080/submit");
    warp::serve(routes.with(warp::log("isixhosa")))
        .run(([0, 0, 0, 0], 8080))
        .await;
}


#[derive(Deserialize, Clone)]
struct SearchQuery {
    query: String,
}

#[derive(Template)]
#[template(path = "index.html")]
struct MainPage;

#[derive(Template)]
#[template(path = "404.html")]
struct NotFound;

#[derive(Template, Default)]
#[template(path = "search.html")]
struct Search {
    hits: Vec<ShortWordSearchHit>,
    query: String,
}

async fn query_search(
    query: SearchQuery,
    typesense: TypesenseClient,
) -> Result<impl warp::Reply, Rejection> {
    let results = typesense.search_word_short(&query.query).await.unwrap();

    Ok(Search {
        query: query.query,
        hits: results.hits,
    })
}

fn live_search(ws: warp::ws::Ws, typesense: TypesenseClient) -> impl warp::Reply {
    ws.on_upgrade(move |websocket| {
        let (sender, stream) = websocket.split();
        let addr = LiveSearchSession::new(sender, typesense)
            .create(Some(4))
            .spawn_global();
        tokio::spawn(addr.attach_stream(stream.map(WsMessage)));
        futures::future::ready(())
    })
}
