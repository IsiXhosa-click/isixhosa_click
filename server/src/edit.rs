use std::sync::Arc;

use isixhosa_common::database::{DbBase, UserAccessDb, WordId};
use isixhosa_common::i18n::SiteContext;
use isixhosa_common::templates::WordChangeMethod;
use tracing::instrument;
use warp::{body, Filter, Rejection, Reply};

use crate::auth::{with_user_auth, FullUser};
use crate::database::submit::{submit_suggestion, suggest_word_deletion, WordSubmission};
use crate::details::word;
use crate::i18n::I18nInfo;
use crate::search::TantivyClient;
use crate::serialization::qs_form;
use crate::submit::edit_word_page;
use crate::DebugBoxedExt;

pub fn edit(
    db: DbBase,
    tantivy: Arc<TantivyClient>,
    site_ctx: Arc<SiteContext>,
) -> impl Filter<Error = Rejection, Extract = impl Reply> + Clone {
    let submit_page = warp::get()
        .and(warp::any().map(|| None)) // previous_success is none
        .and(warp::path![u64 / "edit"])
        .and(warp::path::end())
        .and(with_user_auth(db.clone(), site_ctx.clone()))
        .and_then(edit_word_page);

    let submit_form = warp::post()
        .and(warp::path![u64])
        .and(warp::path::end())
        .and(body::content_length_limit(64 * 1024))
        .and(qs_form())
        .and(warp::any().map(move || tantivy.clone()))
        .and(with_user_auth(db.clone(), site_ctx.clone()))
        .and_then(submit_suggestion_reply);

    let failed_to_submit = warp::any()
        .and(warp::any().map(|| Some(false))) // previous_success is Some(false)
        .and(warp::path![u64])
        .and(warp::path::end())
        .and(with_user_auth(db.clone(), site_ctx.clone()))
        .and_then(edit_word_page);

    let delete_redirect = warp::post()
        .and(warp::path![u64 / "delete"])
        .and(warp::path::end())
        .and(with_user_auth(db, site_ctx))
        .and_then(delete_word_reply);

    warp::path("word")
        .and(
            submit_page
                .or(submit_form)
                .or(delete_redirect)
                .or(failed_to_submit),
        )
        .debug_boxed()
}

#[instrument(name = "Submit word edit form", fields(word_id = id), skip_all)]
async fn submit_suggestion_reply(
    id: u64,
    w: WordSubmission,
    tantivy: Arc<TantivyClient>,
    user: FullUser,
    i18n_info: I18nInfo,
    db: impl UserAccessDb,
) -> Result<impl Reply, Rejection> {
    submit_suggestion(w, tantivy, &user, &db, i18n_info.clone()).await;
    word(id, Some(WordChangeMethod::Edit), user.into(), i18n_info, db).await
}

#[instrument(name = "Suggest to delete word", skip(user, db))]
async fn delete_word_reply(
    id: u64,
    user: FullUser,
    i18n_info: I18nInfo,
    db: impl UserAccessDb,
) -> Result<impl Reply, Rejection> {
    suggest_word_deletion(&user, WordId(id), &db).await;
    word(
        id,
        Some(WordChangeMethod::Delete),
        user.into(),
        i18n_info,
        db,
    )
    .await
}
