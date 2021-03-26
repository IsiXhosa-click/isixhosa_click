#![feature(associated_type_bounds)]

// TODO error handling - dont crash always probably
// TODO ratelimiting
// TODO CSS & styling

use crate::typesense::{ShortWordSearchResults, TypesenseClient};
use arcstr::ArcStr;
use askama::Template;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::Deserialize;
use tokio::task;
use warp::{Filter, Rejection, path};

mod language;
mod session;
mod submit;
mod typesense;
mod auth;

use crate::session::{LiveSearchSession, WsMessage};
use futures::StreamExt;
use submit::submit;
use warp::reject::Reject;
use xtra::spawn::TokioGlobalSpawnExt;
use xtra::Actor;

const CREATE_TABLE: &str = include_str!("sql/words.sql");

#[derive(Debug)]
struct TemplateError(askama::Error);

impl Reject for TemplateError {}

#[tokio::main]
async fn main() {
    let manager = SqliteConnectionManager::file("isixhosa_xyz.db");
    let pool = Pool::new(manager).unwrap();
    let pool_clone = pool.clone();
    let typesense = TypesenseClient {
        api_key: ArcStr::from(std::env::var("TYPESENSE_API_KEY").unwrap()),
    };

    task::spawn_blocking(move || {
        pool_clone
            .get()
            .unwrap()
            .execute(CREATE_TABLE, params![])
            .unwrap()
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

    let search_page = warp::any().map(|| ShortWordSearchResults::default());
    let query_search = warp::query()
        .and(typesense_filter.clone())
        .and_then(query_search);
    let live_search = warp::ws().and(typesense_filter).map(live_search);
    let search = warp::path("search").and(path::end()).and(live_search.or(query_search).or(search_page));

    let routes = warp::fs::dir("static")
        .or(search)
        .or(submit(pool, typesense))
        .or(warp::get().and(path::end()).map(|| MainPage))
        .or(warp::fs::file("pages/404.html"));

    warp::serve(routes).run(([127, 0, 0, 1], 8000)).await;
}

#[derive(Deserialize, Clone)]
struct SearchQuery {
    query: String,
}

#[derive(Template)]
#[template(path = "index.html")]
struct MainPage;

async fn query_search(
    query: SearchQuery,
    typesense: TypesenseClient,
) -> Result<impl warp::Reply, Rejection> {
    Ok(typesense.search_word_short(&query.query).await.unwrap())
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
