use crate::database::existing::ExistingExample;
use crate::search::WordHit;
use fallible_iterator::FallibleIterator;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::convert::TryFrom;

#[derive(Debug)]
pub struct WordDeletionSuggestion {
    pub suggestion_id: u64,
    pub word: WordHit,
    pub reason: String,
}

impl WordDeletionSuggestion {
    pub fn fetch_all(db: &Pool<SqliteConnectionManager>) -> Vec<Self> {
        const SELECT: &str =
            "SELECT words.word_id, words.english, words.xhosa, words.part_of_speech, words.is_plural,
                    words.noun_class, word_deletion_suggestions.reason,
                    word_deletion_suggestions.suggestion_id
            FROM words
            INNER JOIN word_deletion_suggestions
            ON words.word_id = word_deletion_suggestions.word_id;";

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

    pub fn fetch_word_id_for_suggestion(
        db: &Pool<SqliteConnectionManager>,
        suggestion: u64,
    ) -> u64 {
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

    pub fn reject(db: &Pool<SqliteConnectionManager>, suggestion: u64) {
        const DELETE: &str = "DELETE FROM word_deletion_suggestions WHERE suggestion_id = ?1";

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

impl ExampleDeletionSuggestion {
    pub fn fetch_all(db: &Pool<SqliteConnectionManager>) -> Vec<Self> {
        const SELECT: &str =
            "SELECT examples.example_id, examples.word_id, examples.xhosa, examples.english,
                    example_deletion_suggestions.suggestion_id, example_deletion_suggestions.reason
            FROM examples
            INNER JOIN example_deletion_suggestions
            ON examples.example_id = example_deletion_suggestions.example_id;";

        let conn = db.get().unwrap();

        // thanks rustc for forcing this `let x = ...; x` very cool
        let x = conn
            .prepare(SELECT)
            .unwrap()
            .query(params![])
            .unwrap()
            .map(|row| {
                Ok(ExampleDeletionSuggestion {
                    suggestion_id: row.get::<&str, i64>("suggestion_id").unwrap() as u64,
                    example: ExistingExample::try_from(row).unwrap(),
                    reason: row.get("reason").unwrap(),
                })
            })
            .collect()
            .unwrap();
        x
    }

    pub fn fetch_example_id_for_suggestion(
        db: &Pool<SqliteConnectionManager>,
        suggestion: u64,
    ) -> u64 {
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

    pub fn reject(db: &Pool<SqliteConnectionManager>, suggestion: u64) {
        const DELETE: &str = "DELETE FROM example_deletion_suggestions WHERE suggestion_id = ?1";

        let conn = db.get().unwrap();
        conn.prepare(DELETE)
            .unwrap()
            .execute(params![suggestion])
            .unwrap();
    }
}
