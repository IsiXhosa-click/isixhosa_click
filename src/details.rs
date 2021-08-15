use crate::database::existing::ExistingWord;
use crate::language::PartOfSpeech;
use crate::NotFound;
use askama::Template;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use warp::{Filter, Rejection, Reply};

pub fn details(
    db: Pool<SqliteConnectionManager>,
) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    let db = warp::any().map(move || db.clone());

    warp::path!["word" / u64]
        .and(warp::path::end())
        .and(warp::get())
        .and(db)
        .and(warp::any().map(|| None)) // previous_success is None
        .and_then(word)
}

#[derive(Template)]
#[template(path = "word_details.html")]
struct WordDetails {
    word: ExistingWord,
    previous_success: Option<WordChangeMethod>,
}

pub async fn word(
    word_id: u64,
    db: Pool<SqliteConnectionManager>,
    previous_success: Option<WordChangeMethod>,
) -> Result<impl warp::Reply, Rejection> {
    Ok(match ExistingWord::fetch_full(&db, word_id) {
        Some(word) => WordDetails {
            word,
            previous_success,
        }
        .into_response(),
        None => NotFound.into_response(),
    })
}

pub enum WordChangeMethod {
    Edit,
    Delete,
}
