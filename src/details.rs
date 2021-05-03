use askama::Template;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use warp::{Filter, Rejection, Reply};
use crate::database::existing::ExistingWord;
use warp::reject::Reject;
use crate::database::get_word_hit_from_db;
use crate::NotFound;

pub fn details(
    db: Pool<SqliteConnectionManager>,
) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    let db = warp::any().map(move || db.clone());

    warp::path![ "word" / i64 ]
        .and(warp::path::end())
        .and(warp::get())
        .and(db)
        .and_then(word)
}

#[derive(Template)]
#[template(path = "word_details.html")]
struct WordDetails {
    word: ExistingWord,
}

async fn word(
    word_id: i64,
    db: Pool<SqliteConnectionManager>,
) -> Result<impl warp::Reply, Rejection> {
    Ok(match ExistingWord::get_full(db, word_id) {
        Some(word) => WordDetails { word }.into_response(),
        None => NotFound.into_response(),
    })
}
