use crate::auth::ModeratorAccessDb;
use crate::database::existing::{ExistingExample, ExistingLinkedWord};
use crate::search::WordHit;
use crate::submit::WordId;
use fallible_iterator::FallibleIterator;

use rusqlite::{params, Row};
use std::collections::HashMap;
use std::convert::TryFrom;

#[derive(Debug)]
pub struct WordDeletionSuggestion {
    pub suggestion_id: u64,
    pub word: WordHit,
    pub reason: String,
}

impl WordDeletionSuggestion {
    pub fn fetch_all(db: &impl ModeratorAccessDb) -> Vec<Self> {
        const SELECT: &str =
            "SELECT words.word_id, words.english, words.xhosa, words.part_of_speech, words.is_plural,
                    words.noun_class, word_deletion_suggestions.reason,
                    word_deletion_suggestions.suggestion_id
            FROM words
            INNER JOIN word_deletion_suggestions
            ON words.word_id = word_deletion_suggestions.word_id
            ORDER BY words.word_id;";

        let conn = db.get().unwrap();

        // thanks rustc for forcing this `let x = ...; x` very cool
        let x = conn
            .prepare(SELECT)
            .unwrap()
            .query(params![])
            .unwrap()
            .map(|row| {
                Ok(WordDeletionSuggestion {
                    suggestion_id: row.get::<&str, i64>("suggestion_id").unwrap() as u64,
                    word: WordHit::try_from_row_and_id(
                        row,
                        row.get::<&str, i64>("word_id").unwrap() as u64,
                    )
                    .unwrap(),
                    reason: row.get("reason").unwrap(),
                })
            })
            .collect()
            .unwrap();
        x
    }

    pub fn fetch_word_id_for_suggestion(db: &impl ModeratorAccessDb, suggestion: u64) -> u64 {
        const SELECT: &str =
            "SELECT word_id FROM word_deletion_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        let word_id = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![suggestion], |row| row.get("word_id"))
            .unwrap();
        word_id
    }

    pub fn reject(db: &impl ModeratorAccessDb, suggestion: u64) {
        const DELETE: &str = "DELETE FROM word_deletion_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        conn.prepare(DELETE)
            .unwrap()
            .execute(params![suggestion])
            .unwrap();
    }
}

#[derive(Debug)]
pub struct ExampleDeletionSuggestion {
    pub suggestion_id: u64,
    pub example: ExistingExample,
    pub reason: String,
}

impl TryFrom<&Row<'_>> for ExampleDeletionSuggestion {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(ExampleDeletionSuggestion {
            suggestion_id: row.get::<&str, i64>("suggestion_id")? as u64,
            example: ExistingExample::try_from(row)?,
            reason: row.get("reason")?,
        })
    }
}

impl ExampleDeletionSuggestion {
    pub fn fetch_all(db: &impl ModeratorAccessDb) -> impl Iterator<Item = (WordId, Vec<Self>)> {
        const SELECT: &str =
            "SELECT examples.example_id, examples.word_id, examples.xhosa, examples.english,
                    example_deletion_suggestions.suggestion_id, example_deletion_suggestions.reason
            FROM examples
            INNER JOIN example_deletion_suggestions
            ON examples.example_id = example_deletion_suggestions.example_id;";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let deletions = query.query(params![]).unwrap();

        let mut map: HashMap<WordId, Vec<Self>> = HashMap::new();

        deletions
            .map(|row| {
                Ok((
                    WordId(row.get::<&str, u64>("word_id")?),
                    ExampleDeletionSuggestion::try_from(row)?,
                ))
            })
            .for_each(|(word_id, deletion)| {
                map.entry(word_id)
                    .or_insert_with(|| Vec::with_capacity(1))
                    .push(deletion);
                Ok(())
            })
            .unwrap();

        map.into_iter()
    }

    fn fetch_example_id_for_suggestion(db: &impl ModeratorAccessDb, suggestion: u64) -> u64 {
        const SELECT: &str =
            "SELECT example_id FROM example_deletion_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        let word_id = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![suggestion], |row| row.get("example_id"))
            .unwrap();
        word_id
    }

    pub fn accept(db: &impl ModeratorAccessDb, suggestion: u64) {
        const DELETE_EXAMPLE: &str = "DELETE FROM examples WHERE example_id = ?1;";

        let to_delete = Self::fetch_example_id_for_suggestion(db, suggestion);
        let conn = db.get().unwrap();
        conn.prepare(DELETE_EXAMPLE)
            .unwrap()
            .execute(params![to_delete])
            .unwrap();
        Self::delete_suggestion(db, suggestion);
    }

    pub fn delete_suggestion(db: &impl ModeratorAccessDb, suggestion: u64) {
        const DELETE: &str = "DELETE FROM example_deletion_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        conn.prepare(DELETE)
            .unwrap()
            .execute(params![suggestion])
            .unwrap();
    }
}

#[derive(Debug)]
pub struct LinkedWordDeletionSuggestion {
    pub suggestion_id: u64,
    pub link: ExistingLinkedWord,
    pub reason: String,
}

impl LinkedWordDeletionSuggestion {
    fn try_from_row_populate_other(
        row: &Row<'_>,
        db: &impl ModeratorAccessDb,
        skip_populating: u64,
    ) -> Result<Self, rusqlite::Error> {
        Ok(LinkedWordDeletionSuggestion {
            suggestion_id: row.get::<&str, i64>("suggestion_id")? as u64,
            link: ExistingLinkedWord::try_from_row_populate_other(row, db, skip_populating)?,
            reason: row.get("reason")?,
        })
    }

    pub fn fetch_all(db: &impl ModeratorAccessDb) -> impl Iterator<Item = (WordId, Vec<Self>)> {
        const SELECT: &str =
            "SELECT linked_words.link_id, linked_words.link_type, linked_words.first_word_id,
                    linked_words.second_word_id, linked_word_deletion_suggestions.suggestion_id,
                    linked_word_deletion_suggestions.reason
            FROM linked_words
            INNER JOIN linked_word_deletion_suggestions
            ON linked_words.link_id = linked_word_deletion_suggestions.linked_word_id;";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let deletions = query.query(params![]).unwrap();

        let mut map: HashMap<WordId, Vec<Self>> = HashMap::new();

        deletions
            .map(|row| {
                // Chosen is mostly arbitrary
                let first_id = row.get::<&str, u64>("first_word_id")?;
                Ok((
                    WordId(first_id),
                    LinkedWordDeletionSuggestion::try_from_row_populate_other(row, db, first_id)?,
                ))
            })
            .for_each(|(word_id, deletion)| {
                map.entry(word_id)
                    .or_insert_with(|| Vec::with_capacity(1))
                    .push(deletion);
                Ok(())
            })
            .unwrap();

        map.into_iter()
    }

    fn fetch_link_id_for_suggestion(db: &impl ModeratorAccessDb, suggestion: u64) -> u64 {
        const SELECT: &str =
            "SELECT linked_word_id FROM linked_word_deletion_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        let word_id = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![suggestion], |row| row.get("linked_word_id"))
            .unwrap();
        word_id
    }

    pub fn accept(db: &impl ModeratorAccessDb, suggestion: u64) {
        const DELETE: &str = "DELETE FROM linked_words WHERE link_id = ?1;";

        let to_delete = Self::fetch_link_id_for_suggestion(db, suggestion);
        let conn = db.get().unwrap();
        conn.prepare(DELETE)
            .unwrap()
            .execute(params![to_delete])
            .unwrap();
        Self::delete_suggestion(db, suggestion);
    }

    pub fn delete_suggestion(db: &impl ModeratorAccessDb, suggestion: u64) {
        const DELETE: &str =
            "DELETE FROM linked_word_deletion_suggestions WHERE suggestion_id = ?1";

        let conn = db.get().unwrap();
        conn.prepare(DELETE)
            .unwrap()
            .execute(params![suggestion])
            .unwrap();
    }
}
