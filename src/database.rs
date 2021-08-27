//! TODO(cleanup) refactor to put all DB stuff here or in a module under here

use std::convert::TryFrom;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::search::WordHit;
use crate::serialization::{NounClassOpt, NounClassOptExt};
use crate::serialization::{SerializeDisplay, SerializePrimitive};
use crate::submit::WordId;

pub mod deletion;
pub mod existing;
pub mod suggestion;

impl WordHit {
    fn try_from_row_and_id(row: &Row<'_>, id: u64) -> Result<WordHit, rusqlite::Error> {
        Ok(WordHit {
            id,
            english: row.get("english")?,
            xhosa: row.get("xhosa")?,
            part_of_speech: SerializeDisplay(row.get("part_of_speech")?),
            is_plural: row.get("is_plural")?,
            noun_class: row
                .get::<&str, Option<NounClassOpt>>("noun_class")?
                .flatten()
                .map(SerializePrimitive::new),
        })
    }

    pub fn fetch_from_db(
        db: &Pool<SqliteConnectionManager>,
        id: WordOrSuggestionId,
    ) -> Option<WordHit> {
        const SELECT_EXISTING: &str =
            "SELECT word_id, english, xhosa, part_of_speech, is_plural, noun_class FROM words
            WHERE word_id = ?1;";
        const SELECT_SUGGESTED: &str =
            "SELECT english, xhosa, part_of_speech, is_plural, noun_class FROM word_suggestions
            WHERE suggestion_id = ?1;";

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
                WordHit::try_from_row_and_id(row, id.inner())
            })
            .optional()
            .unwrap();
        v
    }
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
    fn inner(&self) -> u64 {
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
            (Some(existing_id), None) => Ok(WordOrSuggestionId::ExistingWord { existing_id }),
            (None, Some(suggestion_id)) => Ok(WordOrSuggestionId::Suggested { suggestion_id }),
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
