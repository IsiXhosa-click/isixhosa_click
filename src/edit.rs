use crate::auth::{with_user_auth, DbBase, User, UserAccessDb};
use crate::details::{word, WordChangeMethod};
use crate::serialization::qs_form;
use crate::submit::{
    edit_word_page, submit_suggestion, suggest_word_deletion, WordId, WordSubmission,
};

use crate::search::TantivyClient;
use std::sync::Arc;
use warp::{body, Filter, Rejection, Reply};
use tracing::instrument;

pub fn edit(
    db: DbBase,
    tantivy: Arc<TantivyClient>,
) -> impl Filter<Error = Rejection, Extract = impl Reply> + Clone {
    let submit_page = warp::get()
        .and(with_user_auth(db.clone()))
        .and(warp::any().map(|| None)) // previous_success is none
        .and(warp::path![u64 / "edit"])
        .and(warp::path::end())
        .and_then(edit_word_page);

    let submit_form = warp::post()
        .and(warp::path![u64])
        .and(warp::path::end())
        .and(body::content_length_limit(64 * 1024))
        .and(warp::any().map(move || tantivy.clone()))
        .and(with_user_auth(db.clone()))
        .and(qs_form())
        .and_then(submit_suggestion_reply);

    let failed_to_submit = warp::any()
        .and(with_user_auth(db.clone()))
        .and(warp::any().map(|| Some(false))) // previous_success is Some(false)
        .and(warp::path![u64])
        .and(warp::path::end())
        .and_then(edit_word_page);

    let delete_redirect = warp::post()
        .and(warp::path![u64 / "delete"])
        .and(warp::path::end())
        .and(with_user_auth(db))
        .and_then(delete_word_reply);

    warp::path("word")
        .and(
            submit_page
                .or(submit_form)
                .or(delete_redirect)
                .or(failed_to_submit),
        )
        .boxed()
}

#[instrument(name = "Submit word edit form", fields(word_id = id), skip_all)]
async fn submit_suggestion_reply(
    id: u64,
    tantivy: Arc<TantivyClient>,
    user: User,
    db: impl UserAccessDb,
    w: WordSubmission,
) -> Result<impl Reply, Rejection> {
    submit_suggestion(w, tantivy, &user, &db).await;
    word(id, user.into(), db, Some(WordChangeMethod::Edit)).await
}

#[instrument(name = "Suggest to delete word", skip(user, db))]
async fn delete_word_reply(
    id: u64,
    user: User,
    db: impl UserAccessDb,
) -> Result<impl Reply, Rejection> {
    suggest_word_deletion(WordId(id), &db).await;
    word(id, user.into(), db, Some(WordChangeMethod::Delete)).await
}
