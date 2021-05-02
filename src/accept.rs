use crate::database::suggestion::MaybeEdited;
use crate::database::suggestion::{get_full_suggested_word, get_suggestions_full, SuggestedWord};
use crate::submit::{edit_suggestion_page, qs_form, submit_suggestion, WordSubmission};
use askama::Template;
use askama_warp::warp::body;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::Deserialize;
use warp::{Filter, Rejection, Reply};
use crate::database::existing::{accept_word_suggestion, accept_new_word_suggestion};
use crate::typesense::{TypesenseClient, WordHit, WordDocument};
use warp::reject::Reject;
use crate::database::get_word_hit_from_db;
use futures::TryFutureExt;

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
}

#[derive(Deserialize)]
struct AcceptParams {
    suggestion: i64,
    method: Method,
}

pub fn accept(
    db: Pool<SqliteConnectionManager>,
    typesense: TypesenseClient,
) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    let db = warp::any().map(move || db.clone());
    let typesense = warp::any().map(move || typesense.clone());

    let show_all = warp::get()
        .and(db.clone())
        .and(warp::any().map(|| None)) // previous_success is None
        .and_then(suggested_words);

    let process_one = warp::post()
        .and(db.clone())
        .and(typesense)
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

    let accept_failed = warp::any()
        .and(db)
        .and(warp::any().map(|| {
            Some(Success {
                success: false,
                method: Some(Method::Accept),
            })
        }))
        .and_then(suggested_words);

    // TODO accept form submit too

    let root = warp::path::end().and(show_all.or(process_one).or(accept_failed));
    let submit_edit = warp::path("edit")
        .and(warp::path::end())
        .and(submit_edit.or(edit_failed));

    warp::path("accept").and(root.or(submit_edit))
}

async fn suggested_words(
    db: Pool<SqliteConnectionManager>,
    previous_success: Option<Success>,
) -> Result<impl warp::Reply, Rejection> {
    let suggestions = tokio::task::spawn_blocking(move || get_suggestions_full(db))
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
    submit_suggestion(submission, db.clone()).await;
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
    db: Pool<SqliteConnectionManager>,
    typesense: TypesenseClient,
    suggestion: i64,
) -> Result<impl Reply, Rejection> {
    let db_clone = db.clone();
    let (word, id) = tokio::task::spawn_blocking(move || {
        let word = get_full_suggested_word(db.clone(), suggestion).unwrap();
        (word.clone(), accept_new_word_suggestion(db.clone(), word))
    }).await.unwrap();

    let res = typesense
        .add_word(WordDocument {
            id: id.to_string(),
            english: word.english.current().clone(),
            xhosa: word.xhosa.current().clone(),
            part_of_speech: *word.part_of_speech.current(),
            is_plural: *word.is_plural.current(),
            noun_class: *word.noun_class.current(),
        })
        .inspect_err(|e| eprintln!("Error adding a word to typesense: {:#?}", e))
        .await;

    suggested_words(
        db_clone,
        Some(Success {
            success: res.is_ok(),
            method: Some(Method::Accept),
        }),
    )
    .await
}

async fn process_one(
    db: Pool<SqliteConnectionManager>,
    typesense: TypesenseClient,
    params: AcceptParams,
) -> Result<impl Reply, Rejection> {
    match params.method {
        Method::Edit => edit_suggestion_page(db, params.suggestion)
            .await
            .map(Reply::into_response),
        Method::Accept => accept_suggestion(db, typesense, params.suggestion)
            .await
            .map(Reply::into_response),
    }
}
