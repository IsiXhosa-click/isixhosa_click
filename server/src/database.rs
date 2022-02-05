//! TODO(cleanup) refactor to put all DB stuff here or in a module under here

use rusqlite::params;
use tracing::instrument;
use isixhosa_common::database::{ModeratorAccessDb, WordId, WordOrSuggestionId};
use isixhosa_common::types::PublicUserInfo;

pub mod deletion;
pub mod submit;
pub mod suggestion;
pub mod user;

#[instrument(name = "Add attribution", skip(db))]
pub fn add_attribution(db: &impl ModeratorAccessDb, user: &PublicUserInfo, word: WordId) {
    const INSERT: &str =
        "INSERT INTO user_attributions (user_id, word_id) VALUES (?1, ?2) ON CONFLICT DO NOTHING;";

    db.get()
        .unwrap()
        .prepare(INSERT)
        .unwrap()
        .execute(params![user.id.get(), word.0])
        .unwrap();
}
