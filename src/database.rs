//! TODO(cleanup) refactor to put all DB stuff here or in a module under here

use std::convert::TryFrom;

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::auth::{ModeratorAccessDb, PublicAccessDb, PublicUserInfo};
use crate::search::WordHit;
use crate::serialization::GetWithSentinelExt;
use crate::serialization::SerOnlyDisplay;
use crate::submit::WordId;
use crate::language::NounClassExt;
use isixhosa::noun::NounClass;

pub mod deletion;
pub mod existing;
pub mod suggestion;
pub mod user;

impl WordHit {
    fn try_from_row_and_id(
        row: &Row<'_>,
        id: WordOrSuggestionId,
    ) -> Result<WordHit, rusqlite::Error> {
        Ok(WordHit {
            id: id.inner(),
            english: row.get("english")?,
            xhosa: row.get("xhosa")?,
            part_of_speech: SerOnlyDisplay(row.get("part_of_speech")?),
            is_plural: row.get("is_plural")?,
            is_inchoative: row.get("is_inchoative")?,
            transitivity: row.get_with_sentinel("transitivity")?.map(SerOnlyDisplay),
            is_suggestion: id.is_suggested(),
            noun_class: row.get_with_sentinel("noun_class")?.map(|c: NounClass| c.to_prefixes()),
        })
    }

    pub fn fetch_from_db(db: &impl PublicAccessDb, id: WordOrSuggestionId) -> Option<WordHit> {
        const SELECT_EXISTING: &str = "
            SELECT
                english, xhosa, part_of_speech, is_plural, is_inchoative, transitivity, noun_class
            FROM words
            WHERE word_id = ?1;
        ";
        const SELECT_SUGGESTED: &str = "
            SELECT
                english, xhosa, part_of_speech, is_plural, is_inchoative, transitivity, noun_class,
                username, display_name, suggesting_user
            FROM word_suggestions
            INNER JOIN users ON word_suggestions.suggesting_user = users.user_id
            WHERE suggestion_id = ?1;
        ";

        let conn = db.get().unwrap();

        let stmt = match id {
            WordOrSuggestionId::ExistingWord { .. } => SELECT_EXISTING,
            WordOrSuggestionId::Suggested { .. } => SELECT_SUGGESTED,
        };

        // WTF rustc?
        #[allow(clippy::redundant_closure)] // implementation of FnOnce is not general enough
        let v = conn
            .prepare(stmt)
            .unwrap()
            .query_row(params![id.inner()], |row| {
                WordHit::try_from_row_and_id(row, id)
            })
            .optional()
            .unwrap();
        v
    }
}

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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WordOrSuggestionId {
    ExistingWord { existing_id: u64 },
    Suggested { suggestion_id: u64 },
}

impl From<WordId> for WordOrSuggestionId {
    fn from(id: WordId) -> Self {
        WordOrSuggestionId::ExistingWord { existing_id: id.0 }
    }
}

impl WordOrSuggestionId {
    pub fn suggested(id: u64) -> WordOrSuggestionId {
        WordOrSuggestionId::Suggested { suggestion_id: id }
    }

    pub fn existing(id: u64) -> WordOrSuggestionId {
        WordOrSuggestionId::ExistingWord { existing_id: id }
    }

    pub fn into_existing(self) -> Option<u64> {
        match self {
            WordOrSuggestionId::ExistingWord { existing_id } => Some(existing_id),
            _ => None,
        }
    }

    pub fn into_suggested(self) -> Option<u64> {
        match self {
            WordOrSuggestionId::Suggested { suggestion_id } => Some(suggestion_id),
            _ => None,
        }
    }

    pub fn is_existing(&self) -> bool {
        matches!(self, WordOrSuggestionId::ExistingWord { .. })
    }

    pub fn is_suggested(&self) -> bool {
        !self.is_existing()
    }

    pub fn inner(&self) -> u64 {
        match self {
            WordOrSuggestionId::ExistingWord { existing_id } => *existing_id,
            WordOrSuggestionId::Suggested { suggestion_id } => *suggestion_id,
        }
    }

    fn try_from_row(
        row: &Row<'_>,
        existing_idx: &str,
        suggested_idx: &str,
    ) -> Result<WordOrSuggestionId, rusqlite::Error> {
        let existing_word_id: Option<u64> = row
            .get::<&str, Option<i64>>(existing_idx)
            .unwrap()
            .map(|x| x as u64);
        let suggested_word_id: Option<u64> = row
            .get::<&str, Option<i64>>(suggested_idx)
            .unwrap()
            .map(|x| x as u64);
        match (existing_word_id, suggested_word_id) {
            (Some(existing_id), None) => Ok(WordOrSuggestionId::existing(existing_id)),
            (None, Some(suggestion_id)) => Ok(WordOrSuggestionId::suggested(suggestion_id)),
            (existing, _suggested) => {
                panic!(
                    "Invalid pair of existing/suggested ids: existing - {:?} suggested - {:?}",
                    existing, suggested_word_id
                )
            }
        }
    }
}

impl TryFrom<&Row<'_>> for WordOrSuggestionId {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        WordOrSuggestionId::try_from_row(row, "existing_word_id", "suggested_word_id")
    }
}
