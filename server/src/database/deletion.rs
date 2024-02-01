use crate::database::WordId;
use crate::database::WordOrSuggestionId;
use fallible_iterator::FallibleIterator;
use isixhosa_common::database::ModeratorAccessDb;
use isixhosa_common::types::{ExistingExample, ExistingLinkedWord, PublicUserInfo, WordHit};
use rusqlite::{params, Row};
use std::collections::HashMap;
use std::convert::TryFrom;
use tracing::{instrument, Span};

#[derive(Debug)]
pub struct WordDeletionSuggestion {
    pub suggestion_id: u64,
    pub suggesting_user: PublicUserInfo,
    pub word: WordHit,
    pub reason: String,
}

impl WordDeletionSuggestion {
    #[instrument(
        name = "Fetch all word deletion suggestions",
        fields(results),
        skip_all
    )]
    pub fn fetch_all(db: &impl ModeratorAccessDb) -> Vec<Self> {
        const SELECT: &str =
            "SELECT words.word_id, words.english, words.xhosa, words.part_of_speech, words.is_plural,
                    words.is_inchoative, words.is_informal, words.transitivity, words.followed_by,
                    words.noun_class, word_deletion_suggestions.reason, word_deletion_suggestions.suggestion_id,
                    users.username, users.display_name, word_deletion_suggestions.suggesting_user
            FROM words
            INNER JOIN word_deletion_suggestions
                ON words.word_id = word_deletion_suggestions.word_id
            INNER JOIN users ON word_deletion_suggestions.suggesting_user = users.user_id
            ORDER BY words.word_id;";

        let conn = db.get().unwrap();

        // thanks rustc for forcing this `let x = ...; x` very cool
        let x: Vec<Self> = conn
            .prepare(SELECT)
            .unwrap()
            .query(params![])
            .unwrap()
            .map(|row| {
                Ok(WordDeletionSuggestion {
                    suggestion_id: row.get::<&str, i64>("suggestion_id")? as u64,
                    suggesting_user: PublicUserInfo::try_from(row)?,
                    word: WordHit::try_from_row_and_id(
                        row,
                        WordOrSuggestionId::existing(row.get::<&str, i64>("word_id")? as u64),
                    )
                    .unwrap(),
                    reason: row.get("reason")?,
                })
            })
            .collect()
            .unwrap();

        Span::current().record("results", &x.len());

        x
    }

    #[instrument(
        name = "Fetch word id for deletion suggestion",
        fields(word_id),
        skip(db)
    )]
    pub fn fetch_word_id_for_suggestion(db: &impl ModeratorAccessDb, suggestion: u64) -> u64 {
        const SELECT: &str =
            "SELECT word_id FROM word_deletion_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        let word_id = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![suggestion], |row| row.get("word_id"))
            .unwrap();

        Span::current().record("word_id", word_id);

        word_id
    }

    #[instrument(name = "Reject a deletion suggestion", skip(db))]
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
    pub suggesting_user: PublicUserInfo,
    pub example: ExistingExample,
    pub reason: String,
}

impl TryFrom<&Row<'_>> for ExampleDeletionSuggestion {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(ExampleDeletionSuggestion {
            suggestion_id: row.get::<&str, i64>("suggestion_id")? as u64,
            suggesting_user: PublicUserInfo::try_from(row)?,
            example: ExistingExample::try_from(row)?,
            reason: row.get("reason")?,
        })
    }
}

impl ExampleDeletionSuggestion {
    #[instrument(
        name = "Fetch all example deletions suggestions",
        fields(results),
        skip(db)
    )]
    pub fn fetch_all(db: &impl ModeratorAccessDb) -> impl Iterator<Item = (WordId, Vec<Self>)> {
        const SELECT: &str =
            "SELECT examples.example_id, examples.word_id, examples.xhosa, examples.english,
                    example_deletion_suggestions.suggestion_id, example_deletion_suggestions.reason,
                    users.username, users.display_name, example_deletion_suggestions.suggesting_user
            FROM examples
            INNER JOIN users ON example_deletion_suggestions.suggesting_user = users.user_id
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

        Span::current().record("results", &map.len());

        map.into_iter()
    }

    #[instrument(
        name = "Fetch example id for example deletion suggestion",
        fields(example_id),
        skip(db)
    )]
    fn fetch_example_id_for_suggestion(db: &impl ModeratorAccessDb, suggestion: u64) -> u64 {
        const SELECT: &str =
            "SELECT example_id FROM example_deletion_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        let example_id = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![suggestion], |row| row.get("example_id"))
            .unwrap();

        Span::current().record("example_id", &example_id);

        example_id
    }

    #[instrument(name = "Accept example deletion suggestion", skip(db))]
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

    #[instrument(name = "Delete example deletion suggestion", skip(db))]
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
    pub suggesting_user: PublicUserInfo,
    pub link: ExistingLinkedWord,
    pub reason: String,
}

impl LinkedWordDeletionSuggestion {
    #[instrument(
        name = "Populate linked word deletion suggestion",
        fields(suggestion_id),
        skip(row, db)
    )]
    fn try_from_row_populate_other(
        row: &Row<'_>,
        db: &impl ModeratorAccessDb,
        skip_populating: u64,
    ) -> Result<Self, rusqlite::Error> {
        let suggestion_id = row.get::<&str, i64>("suggestion_id")? as u64;

        Span::current().record("suggestion_id", &suggestion_id);

        Ok(LinkedWordDeletionSuggestion {
            suggestion_id,
            suggesting_user: PublicUserInfo::try_from(row)?,
            link: ExistingLinkedWord::try_from_row_populate_other(row, db, skip_populating)?,
            reason: row.get("reason")?,
        })
    }

    #[instrument(
        name = "Fetch all linked word deletion suggestions",
        fields(results),
        skip(db)
    )]
    pub fn fetch_all(db: &impl ModeratorAccessDb) -> impl Iterator<Item = (WordId, Vec<Self>)> {
        const SELECT: &str =
            "SELECT linked_words.link_id, linked_words.link_type, linked_words.first_word_id,
                    linked_words.second_word_id, linked_word_deletion_suggestions.suggestion_id,
                    linked_word_deletion_suggestions.reason, users.username, users.display_name,
                    linked_word_deletion_suggestions.suggesting_user
            FROM linked_words
            INNER JOIN users ON linked_word_deletion_suggestions.suggesting_user = users.user_id
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

        Span::current().record("results", &map.len());

        map.into_iter()
    }

    #[instrument(
        name = "Fetch link id for linked word deletion suggestion",
        fields(link_id),
        skip(db)
    )]
    fn fetch_link_id_for_suggestion(db: &impl ModeratorAccessDb, suggestion: u64) -> u64 {
        const SELECT: &str =
            "SELECT linked_word_id FROM linked_word_deletion_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        let link_id = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![suggestion], |row| row.get("linked_word_id"))
            .unwrap();

        Span::current().record("link_id", &link_id);

        link_id
    }

    #[instrument(name = "Accept linked word deletion suggestion", skip(db))]
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

    #[instrument(name = "Delete linked word deletion suggestion", skip(db))]
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
