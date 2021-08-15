#![allow(clippy::cmp_owned)] // * is a syntax error in askama so clone has to be used

use crate::database::accept_whole_word_suggestion;
use crate::database::deletion::WordDeletionSuggestion;
use crate::database::existing::ExistingWord;
use crate::database::suggestion::{MaybeEdited, SuggestedWord};
use crate::search::{TantivyClient, WordDocument};
use crate::submit::{edit_suggestion_page, qs_form, submit_suggestion, WordSubmission};
use askama::Template;
use askama_warp::warp::body;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::Deserialize;
use std::sync::Arc;
use warp::{Filter, Rejection, Reply};
use serde_with::{serde_as, DisplayFromStr};

#[derive(Template, Debug)]
#[template(path = "moderation.html")]
struct ModerationTemplate {
    previous_success: Option<Success>,
    word_suggestions: Vec<SuggestedWord>,
    word_deletions: Vec<WordDeletionSuggestion>,
}

#[derive(Debug)]
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
struct Action {
    #[serde(flatten)]
    suggestion: ActionTarget,
    method: Method,
}

#[serde_as]
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "suggestion_type", content = "suggestion")]
enum ActionTarget {
    WordDeletionSuggestion(#[serde_as(as = "DisplayFromStr")] u64),
    WordSuggestion(#[serde_as(as = "DisplayFromStr")] u64),
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
        .and(warp::body::form::<Action>())
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

    warp::path("moderation").and(root.or(submit_edit))
}

async fn suggested_words(
    db: Pool<SqliteConnectionManager>,
    previous_success: Option<Success>,
) -> Result<impl warp::Reply, Rejection> {
    let db_clone = db.clone();
    let (word_suggestions, word_deletions) = tokio::task::spawn_blocking(move || {
        (
            SuggestedWord::fetch_all_full(&db_clone),
            WordDeletionSuggestion::fetch_all(&db_clone),
        )
    })
    .await
    .unwrap();

    Ok(ModerationTemplate {
        previous_success,
        word_suggestions,
        word_deletions,
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

async fn accept_suggested_word(
    db: &Pool<SqliteConnectionManager>,
    tantivy: Arc<TantivyClient>,
    suggestion: u64,
) -> Result<impl Reply, Rejection> {
    let (db, db_clone) = (db.clone(), db.clone());
    let (word, id) = tokio::task::spawn_blocking(move || {
        let word = SuggestedWord::fetch_full(&db, suggestion).unwrap();
        (word.clone(), accept_whole_word_suggestion(&db, word))
    })
    .await
    .unwrap();

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

    suggested_words(
        db_clone,
        Some(Success {
            success: true,
            method: Some(Method::Accept),
        }),
    )
    .await
}

async fn reject_suggested_word(
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

async fn accept_deletion(
    db: &Pool<SqliteConnectionManager>,
    tantivy: Arc<TantivyClient>,
    suggestion: u64,
) -> Result<impl Reply, Rejection> {
    let word_id = WordDeletionSuggestion::fetch_word_id_for_suggestion(db, suggestion);
    let (db, db_clone) = (db.clone(), db.clone());
    tokio::task::spawn_blocking(move || ExistingWord::delete(&db, word_id))
        .await
        .unwrap();
    tantivy.delete_word(word_id).await;

    suggested_words(
        db_clone,
        Some(Success {
            success: true,
            method: Some(Method::Accept),
        }),
    )
    .await
}

async fn reject_deletion(
    db: &Pool<SqliteConnectionManager>,
    suggestion: u64,
) -> Result<impl Reply, Rejection> {
    let (db, db_clone) = (db.clone(), db.clone());
    tokio::task::spawn_blocking(move || WordDeletionSuggestion::reject(&db, suggestion))
        .await
        .unwrap();

    suggested_words(
        db_clone,
        Some(Success {
            success: true,
            method: Some(Method::Reject),
        }),
    )
    .await
}

async fn process_one(
    db: Pool<SqliteConnectionManager>,
    tantivy: Arc<TantivyClient>,
    params: Action,
) -> Result<impl Reply, Rejection> {
    match params.suggestion {
        ActionTarget::WordDeletionSuggestion(suggestion) => match params.method {
            Method::Edit => {
                log::warn!(
                    "Got request to edit word deletion suggestion, but this makes no sense!\
                        Returning error and ignoring."
                );

                suggested_words(
                    db,
                    Some(Success {
                        success: false,
                        method: Some(Method::Edit),
                    }),
                )
                .await
                .map(Reply::into_response)
            }
            Method::Accept => accept_deletion(&db, tantivy, suggestion)
                .await
                .map(Reply::into_response),
            Method::Reject => reject_deletion(&db, suggestion)
                .await
                .map(Reply::into_response),
        },
        ActionTarget::WordSuggestion(suggestion) => match params.method {
            Method::Edit => edit_suggestion_page(db, suggestion)
                .await
                .map(Reply::into_response),
            Method::Accept => accept_suggested_word(&db, tantivy, suggestion)
                .await
                .map(Reply::into_response),
            Method::Reject => reject_suggested_word(&db, suggestion)
                .await
                .map(Reply::into_response),
        },
    }
}
