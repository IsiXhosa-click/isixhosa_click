use crate::auth::with_any_auth;
use crate::i18n::I18nInfo;
use crate::i18n::SiteContext;
use crate::{spawn_blocking_child, DebugBoxedExt, NotFound};
use isixhosa_common::auth::Auth;
use isixhosa_common::database::{DbBase, PublicAccessDb};
use isixhosa_common::templates::{WordChangeMethod, WordDetails};
use isixhosa_common::types::ExistingWord;
use std::sync::Arc;
use tracing::instrument;
use warp::{Filter, Rejection, Reply};

pub fn details(
    db: DbBase,
    site_ctx: Arc<SiteContext>,
) -> impl Filter<Error = Rejection, Extract = impl Reply> + Clone {
    warp::path!["word" / u64]
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::any().map(|| None)) // previous_success is None
        .and(with_any_auth(db, site_ctx))
        .and_then(word)
        .debug_boxed()
}

#[instrument(name = "Display word details page", skip(auth, db, previous_success))]
pub async fn word(
    word_id: u64,
    previous_success: Option<WordChangeMethod>,
    auth: Auth,
    i18n_info: I18nInfo,
    db: impl PublicAccessDb,
) -> Result<impl Reply, Rejection> {
    let db = db.clone();
    let word = spawn_blocking_child(move || ExistingWord::fetch_full(&db, word_id))
        .await
        .unwrap();
    Ok(match word {
        Some(word) => WordDetails {
            auth,
            i18n_info,
            word,
            previous_success,
        }
        .into_response(),
        None => NotFound { auth, i18n_info }.into_response(),
    })
}
