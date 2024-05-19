use crate::auth::{with_user_auth, FullUser};
use crate::database::submit;
use crate::database::submit::{WordFormTemplate, WordSubmission};
use crate::database::suggestion::SuggestedWord;
use crate::i18n::I18nInfo;
use crate::search::TantivyClient;
use crate::serialization::qs_form;
use crate::{spawn_blocking_child, DebugBoxedExt, SiteContext};
use askama::Template;
use isixhosa_common::auth::{Auth, Permissions};
use isixhosa_common::database::{DbBase, UserAccessDb};
use isixhosa_common::format::DisplayHtml;
use isixhosa_common::language::{NounClassExt, Transitivity};
use serde::Deserialize;
use std::fmt::{self, Debug, Display, Formatter};
use std::sync::Arc;
use tracing::instrument;
use warp::{body, path, Filter, Rejection, Reply};

#[derive(Template, Debug)]
#[template(path = "submit.askama.html")]
struct SubmitTemplate {
    auth: Auth,
    previous_success: Option<bool>,
    action: SubmitFormAction,
    word: WordFormTemplate,
}

impl SubmitTemplate {
    fn this_word_id_js(&self) -> String {
        match self.action {
            SubmitFormAction::EditExisting(existing) => existing.to_string(),
            SubmitFormAction::EditSuggestion { suggestion_id, .. } => suggestion_id.to_string(),
            _ => "null".to_owned(),
        }
    }
}

#[derive(Deserialize, Debug, Default, Copy, Clone)]
enum SubmitFormAction {
    EditSuggestion {
        suggestion_id: u64,
        existing_id: Option<u64>,
        suggestion_anchor_ord: u32,
    },
    #[default]
    SubmitNewWord,
    EditExisting(u64),
}

impl Display for SubmitFormAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub fn submit(
    db: DbBase,
    tantivy: Arc<TantivyClient>,
    site_ctx: SiteContext,
) -> impl Filter<Error = Rejection, Extract = impl Reply> + Clone {
    let submit_page = warp::get()
        .and(warp::any().map(|| None)) // previous_success is none
        .and(warp::any().map(SubmitFormAction::default))
        .and(with_user_auth(db.clone(), site_ctx.clone()))
        .and_then(submit_word_page);

    let submit_form = body::content_length_limit(64 * 1024)
        .and(warp::any().map(move || tantivy.clone()))
        .and(qs_form())
        .and(with_user_auth(db.clone(), site_ctx.clone()))
        .and_then(submit_new_word_form);

    let failed_to_submit = warp::any()
        .and(warp::any().map(|| Some(false))) // previous_success is Some(false)
        .and(warp::any().map(SubmitFormAction::default))
        .and(with_user_auth(db, site_ctx.clone()))
        .and_then(submit_word_page);

    let submit_routes = submit_page.or(submit_form).or(failed_to_submit);

    warp::path("submit")
        .and(path::end())
        .and(submit_routes)
        .debug_boxed()
}

#[instrument(name = "Display edit suggestion page", skip(db, user))]
pub async fn edit_suggestion_page(
    db: impl UserAccessDb,
    i18n_info: I18nInfo,
    user: FullUser,
    suggestion_id: u64,
    suggestion_anchor_ord: u32,
) -> Result<impl Reply, Rejection> {
    let db_clone = db.clone();
    let existing_id = spawn_blocking_child(move || {
        SuggestedWord::fetch_existing_id_for_suggestion(&db_clone, suggestion_id)
    })
    .await
    .unwrap();

    submit_word_page(
        None,
        SubmitFormAction::EditSuggestion {
            suggestion_id,
            existing_id,
            suggestion_anchor_ord,
        },
        user,
        i18n_info,
        db,
    )
    .await
}

#[instrument(name = "Display edit word page", skip(user, db, previous_success))]
pub async fn edit_word_page(
    previous_success: Option<bool>,
    id: u64,
    user: FullUser,
    i18n_info: I18nInfo,
    db: impl UserAccessDb,
) -> Result<impl Reply, Rejection> {
    submit_word_page(
        previous_success,
        SubmitFormAction::EditExisting(id),
        user,
        i18n_info,
        db,
    )
    .await
}

// TODO(form validation): server side form validation
#[instrument(name = "Display submit word page", skip_all)]
async fn submit_word_page(
    previous_success: Option<bool>,
    action: SubmitFormAction,
    user: FullUser,
    _i18n_info: I18nInfo,
    db: impl UserAccessDb,
) -> Result<impl Reply, Rejection> {
    let db = db.clone();
    let word = spawn_blocking_child(move || match action {
        SubmitFormAction::EditSuggestion {
            suggestion_id,
            existing_id,
            ..
        } => WordFormTemplate::fetch_from_db(&db, existing_id, Some(suggestion_id))
            .unwrap_or_default(),
        SubmitFormAction::EditExisting(id) => {
            WordFormTemplate::fetch_from_db(&db, Some(id), None).unwrap_or_default()
        }
        SubmitFormAction::SubmitNewWord => WordFormTemplate::default(),
    })
    .await
    .unwrap();

    Ok(SubmitTemplate {
        auth: user.into(),
        previous_success,
        action,
        word,
    })
}

#[instrument(name = "Submit word form", skip_all)]
async fn submit_new_word_form(
    tantivy: Arc<TantivyClient>,
    word: WordSubmission,
    user: FullUser,
    i18n_info: I18nInfo,
    db: impl UserAccessDb,
) -> Result<impl warp::Reply, Rejection> {
    submit::submit_suggestion(word, tantivy, &user, &db).await;
    submit_word_page(
        Some(true),
        SubmitFormAction::SubmitNewWord,
        user,
        i18n_info,
        db,
    )
    .await
}
