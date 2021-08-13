use crate::database::{accept_whole_word_suggestion, delete_word};
use crate::database::suggestion::{MaybeEdited, SuggestedWord};
use crate::submit::{edit_suggestion_page, qs_form, submit_suggestion, WordSubmission, WordId};
use crate::search::{TantivyClient, WordDocument};
use askama::Template;
use askama_warp::warp::body;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::Deserialize;
use warp::{Filter, Rejection, Reply};
use std::sync::Arc;

#[derive(Template)]
#[template(path = "accept.html")]
struct AcceptTemplate {
    previous_success: Option<Success>,
    suggestions: Vec<SuggestedWord>,
}

struct Success {
    success: bool,
    method: Option<Method>,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Method {
    Edit,
    Accept,
    Reject,
}

#[derive(Deserialize)]
struct AcceptParams {
    suggestion: u64,
    method: Method,
}

pub fn accept(
    db: Pool<SqliteConnectionManager>,
    tantivy: Arc<TantivyClient>,
) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    let db = warp::any().map(move || db.clone());
    let tantivy = warp::any().map(move || tantivy.clone());

    let show_all = warp::get()
        .and(db.clone())
        .and(warp::any().map(|| None)) // previous_success is None
        .and_then(suggested_words);

    let process_one = warp::post()
        .and(db.clone())
        .and(tantivy)
        .and(warp::body::form::<AcceptParams>())
        .and_then(process_one);

    let submit_edit = warp::post()
        .and(body::content_length_limit(4 * 1024))
        .and(db.clone())
        .and(qs_form())
        .and_then(edit_suggestion_form);

    let edit_failed = warp::any()
        .and(db.clone())
        .and(warp::any().map(|| {
            Some(Success {
                success: false,
                method: Some(Method::Edit),
            })
        }))
        .and_then(suggested_words);

    let other_failed = warp::any()
        .and(db)
        .and(warp::any().map(|| {
            Some(Success {
                success: false,
                method: None,
            })
        }))
        .and_then(suggested_words);

    let root = warp::path::end().and(show_all.or(process_one).or(other_failed));
    let submit_edit = warp::path("edit")
        .and(warp::path::end())
        .and(submit_edit.or(edit_failed));

    warp::path("accept").and(root.or(submit_edit))
}

async fn suggested_words(
    db: Pool<SqliteConnectionManager>,
    previous_success: Option<Success>,
) -> Result<impl warp::Reply, Rejection> {
    let db_clone = db.clone();
    let suggestions = tokio::task::spawn_blocking(move || SuggestedWord::get_all_full(&db_clone))
        .await
        .unwrap();
    Ok(AcceptTemplate {
        previous_success,
        suggestions,
    })
}

async fn edit_suggestion_form(
    db: Pool<SqliteConnectionManager>,
    submission: WordSubmission,
) -> Result<impl Reply, Rejection> {
    submit_suggestion(submission, &db).await;
    suggested_words(
        db,
        Some(Success {
            success: true,
            method: Some(Method::Edit),
        }),
    )
    .await
}

async fn accept_suggestion(
    db: &Pool<SqliteConnectionManager>,
    tantivy: Arc<TantivyClient>,
    suggestion: u64,
) -> Result<impl Reply, Rejection> {
    let (db, db_clone) = (db.clone(), db.clone());
    let suggestion_and_id = tokio::task::spawn_blocking(move || {
        let word = SuggestedWord::get_full(&db, suggestion).unwrap();

        if !word.deletion {
            (Some(word.clone()), accept_whole_word_suggestion(&db, word))
        } else {
            let word_id = word.word_id.unwrap();
            delete_word(&db, WordId(word_id));
            SuggestedWord::delete(&db, word.suggestion_id);
            (None, word_id)
        }
    })
    .await
    .unwrap();

    match suggestion_and_id {
        (Some(word), id) => {
            let document = WordDocument {
                id: id as u64,
                english: word.english.current().clone(),
                xhosa: word.xhosa.current().clone(),
                part_of_speech: *word.part_of_speech.current(),
                is_plural: *word.is_plural.current(),
                noun_class: *word.noun_class.current(),
            };

            if word.word_id.is_none() {
                tantivy.add_new_word(document).await
            } else {
                tantivy.edit_word(document).await
            }
        },
        (None, id) => tantivy.delete_word(id).await,
    }

    suggested_words(
        db_clone,
        Some(Success {
            success: true,
            method: Some(Method::Accept),
        }),
    )
    .await
}

async fn reject_suggestion(
    db: &Pool<SqliteConnectionManager>,
    suggestion: u64,
) -> Result<impl Reply, Rejection> {
    let (db, db_clone) = (db.clone(), db.clone());
    let success = tokio::task::spawn_blocking(move || SuggestedWord::delete(&db, suggestion))
        .await
        .unwrap();

    suggested_words(
        db_clone,
        Some(Success {
            success,
            method: Some(Method::Reject),
        }),
    )
    .await
}

async fn process_one(
    db: Pool<SqliteConnectionManager>,
    tantivy: Arc<TantivyClient>,
    params: AcceptParams,
) -> Result<impl Reply, Rejection> {
    match params.method {
        Method::Edit => edit_suggestion_page(db, params.suggestion)
            .await
            .map(Reply::into_response),
        Method::Accept => accept_suggestion(&db, tantivy, params.suggestion)
            .await
            .map(Reply::into_response),
        Method::Reject => reject_suggestion(&db, params.suggestion)
            .await
            .map(Reply::into_response),
    }
}
