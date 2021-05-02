//! TODO(cleanup) refactor to put all DB stuff here or in a module under here

use crate::language::SerializeDisplay;
use crate::typesense::WordHit;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension, Row};
use std::convert::TryFrom;

pub mod existing;
pub mod suggestion;

pub fn get_word_hit_from_db(db: Pool<SqliteConnectionManager>, id: i64) -> Option<WordHit> {
    const SELECT: &str = "SELECT english, xhosa, part_of_speech, is_plural, noun_class from words
            WHERE word_id = ?1;";

    let conn = db.get().unwrap();

    // WTF rustc?
    let v = conn
        .prepare(SELECT)
        .unwrap()
        .query_row(params![id], |row| {
            Ok(WordHit {
                id: id.to_string(),
                english: row.get("english").unwrap(),
                xhosa: row.get("xhosa").unwrap(),
                part_of_speech: SerializeDisplay(row.get("part_of_speech").unwrap()),
                is_plural: row.get("is_plural").unwrap(),
                noun_class: row
                    .get::<&str, Option<_>>("noun_class")
                    .unwrap()
                    .map(SerializeDisplay),
            })
        })
        .optional()
        .unwrap();
    v
}

pub fn accept_suggestion_full(db: Pool<SqliteConnectionManager>, suggestion_id: i64) {
    const INSERT: &str = "
        INSERT INTO
    ";
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum WordOrSuggestedId {
    ExistingWord(i64),
    Suggested(i64),
}

impl WordOrSuggestedId {
    fn try_from_row(
        row: &Row<'_>,
        existing_idx: &str,
        suggested_idx: &str,
    ) -> Result<WordOrSuggestedId, rusqlite::Error> {
        let existing_word_id: Option<i64> = row.get(existing_idx).unwrap();
        let suggested_word_id: Option<i64> = row.get(suggested_idx).unwrap();
        match (existing_word_id, suggested_word_id) {
            (Some(existing), None) => Ok(WordOrSuggestedId::ExistingWord(existing)),
            (None, Some(suggested)) => Ok(WordOrSuggestedId::Suggested(suggested)),
            (existing, _suggested) => {
                panic!(
                    "Invalid pair of exisitng/suggested ids: existing - {:?} suggested - {:?}",
                    existing, suggested_word_id
                )
            }
        }
    }
}

impl TryFrom<&Row<'_>> for WordOrSuggestedId {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        WordOrSuggestedId::try_from_row(row, "existing_word_id", "suggested_word_id")
    }
}
