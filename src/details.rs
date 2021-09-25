use crate::auth::PublicAccessDb;
use crate::auth::{with_any_auth, Auth, DbBase};
use crate::database::existing::ExistingWord;
use crate::format::DisplayHtml;
use crate::language::PartOfSpeech;
use crate::NotFound;
use askama::Template;
use warp::{Filter, Rejection, Reply};

pub fn details(db: DbBase) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    warp::path!["word" / u64]
        .and(warp::path::end())
        .and(warp::get())
        .and(with_any_auth(db))
        .and(warp::any().map(|| None)) // previous_success is None
        .and_then(word)
        .boxed()
}

#[derive(Template)]
#[template(path = "word_details.askama.html")]
struct WordDetails {
    auth: Auth,
    word: ExistingWord,
    previous_success: Option<WordChangeMethod>,
}

pub async fn word(
    word_id: u64,
    auth: Auth,
    db: impl PublicAccessDb,
    previous_success: Option<WordChangeMethod>,
) -> Result<impl warp::Reply, Rejection> {
    let db = db.clone();
    let word = tokio::task::spawn_blocking(move || ExistingWord::fetch_full(&db, word_id))
        .await
        .unwrap();
    Ok(match word {
        Some(word) => WordDetails {
            auth,
            word,
            previous_success,
        }
        .into_response(),
        None => NotFound { auth }.into_response(),
    })
}

pub enum WordChangeMethod {
    Edit,
    Delete,
}
