use std::convert::TryFrom;

use rusqlite::{params, OptionalExtension, Row};

use crate::auth::{ModeratorAccessDb, PublicAccessDb, PublicUserInfo};
use crate::database::WordOrSuggestionId;
use crate::language::{ConjunctionFollowedBy, PartOfSpeech, Transitivity, WordLinkType};
use crate::search::WordHit;
use crate::serialization::GetWithSentinelExt;
use fallible_iterator::FallibleIterator;
use isixhosa::noun::NounClass;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::{instrument, Span};

#[derive(Debug)]
pub struct ExistingWord {
    pub word_id: u64,

    pub english: String,
    pub xhosa: String,
    pub part_of_speech: PartOfSpeech,

    pub xhosa_tone_markings: String,
    pub infinitive: String,
    pub is_plural: bool,
    pub is_inchoative: bool,
    pub transitivity: Option<Transitivity>,
    pub followed_by: Option<ConjunctionFollowedBy>,
    pub noun_class: Option<NounClass>,
    pub note: String,

    pub examples: Vec<ExistingExample>,
    pub linked_words: Vec<ExistingLinkedWord>,
    pub contributors: Vec<PublicUserInfo>,
}

impl ExistingWord {
    #[instrument(name = "Fetch full existing word", fields(found), skip(db))]
    pub fn fetch_full(db: &impl PublicAccessDb, id: u64) -> Option<ExistingWord> {
        let mut word = ExistingWord::fetch_alone(db, id);
        if let Some(word) = word.as_mut() {
            word.examples = ExistingExample::fetch_all_for_word(db, id);
            word.linked_words = ExistingLinkedWord::fetch_all_for_word(db, id);
            word.contributors = PublicUserInfo::fetch_public_contributors_for_word(db, id);
        }

        Span::current().record("found", &word.is_some());

        word
    }

    #[instrument(level = "trace", name = "Fetch just existing word", fields(found), skip(db))]
    pub fn fetch_alone(db: &impl PublicAccessDb, id: u64) -> Option<ExistingWord> {
        const SELECT_ORIGINAL: &str = "
            SELECT
                word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
                is_inchoative, transitivity, followed_by, noun_class, note
            FROM words
            WHERE word_id = ?1;
        ";

        let conn = db.get().unwrap();

        #[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
        let opt = conn
            .prepare(SELECT_ORIGINAL)
            .unwrap()
            .query_row(params![id], |row| ExistingWord::try_from(row))
            .optional()
            .unwrap();

        Span::current().record("found", &opt.is_some());

        opt
    }

    #[instrument(name = "Delete existing word", fields(found), skip(db))]
    pub fn delete(db: &impl ModeratorAccessDb, id: u64) -> bool {
        const DELETE: &str = "DELETE FROM words WHERE word_id = ?1;";

        let conn = db.get().unwrap();
        let modified_rows = conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
        let found = modified_rows == 1;
        Span::current().record("found", &found);
        found
    }

    #[instrument(name = "Count all existing words", fields(results), skip(db))]
    pub fn count_all(db: &impl PublicAccessDb) -> u64 {
        const COUNT: &str = "SELECT COUNT(1) FROM words;";

        let conn = db.get().unwrap();
        let count = conn
            .prepare(COUNT)
            .unwrap()
            .query_row(params![], |row| row.get(0))
            .unwrap();

        Span::current().record("results", &count);

        count
    }
}

impl TryFrom<&Row<'_>> for ExistingWord {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, rusqlite::Error> {
        Ok(ExistingWord {
            word_id: row.get("word_id")?,
            english: row.get("english")?,
            xhosa: row.get("xhosa")?,
            part_of_speech: row.get("part_of_speech")?,
            xhosa_tone_markings: row.get("xhosa_tone_markings")?,
            infinitive: row.get("infinitive")?,
            is_plural: row.get("is_plural")?,
            is_inchoative: row.get("is_inchoative")?,
            transitivity: row.get_with_sentinel("transitivity")?,
            followed_by: ConjunctionFollowedBy::from_str(&row.get::<&str, String>("followed_by")?)
                .ok(),
            noun_class: row.get_with_sentinel("noun_class")?,
            note: row.get("note")?,
            examples: vec![],
            linked_words: vec![],
            contributors: vec![],
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExistingExample {
    pub example_id: u64,
    pub word_id: u64,

    pub english: String,
    pub xhosa: String,
}

impl ExistingExample {
    #[instrument(level = "trace", name = "Fetch all existing examples for word", fields(results), skip(db))]
    pub fn fetch_all_for_word(db: &impl PublicAccessDb, word_id: u64) -> Vec<ExistingExample> {
        const SELECT: &str =
            "SELECT example_id, word_id, english, xhosa FROM examples WHERE word_id = ?1;";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let rows = query.query(params![word_id]).unwrap();

        #[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
        let examples: Vec<Self> = rows.map(|row| ExistingExample::try_from(row))
            .collect()
            .unwrap();

        Span::current().record("results", &examples.len());

        examples
    }

    #[instrument(name = "Fetch existing example", fields(found), skip(db))]
    pub fn fetch(db: &impl PublicAccessDb, example_id: u64) -> Option<ExistingExample> {
        const SELECT: &str =
            "SELECT example_id, word_id, english, xhosa FROM examples WHERE example_id = ?1;";

        let conn = db.get().unwrap();
        #[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
        let opt = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![example_id], |row| ExistingExample::try_from(row))
            .optional()
            .unwrap();

        Span::current().record("found", &opt.is_some());

        opt
    }
}

impl TryFrom<&Row<'_>> for ExistingExample {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(ExistingExample {
            example_id: row.get("example_id")?,
            word_id: row.get("word_id")?,
            english: row.get("english")?,
            xhosa: row.get("xhosa")?,
        })
    }
}

#[derive(Debug)]
pub struct ExistingLinkedWord {
    pub link_id: u64,
    pub first_word_id: u64,
    pub second_word_id: u64,
    pub link_type: WordLinkType,
    pub other: WordHit,
}

impl ExistingLinkedWord {
    #[instrument(level = "trace", name = "Fetch all existing linked word for word", fields(results), skip(db))]
    pub fn fetch_all_for_word(db: &impl PublicAccessDb, word_id: u64) -> Vec<ExistingLinkedWord> {
        const SELECT: &str = "
            SELECT link_id, link_type, first_word_id, second_word_id FROM linked_words
                WHERE first_word_id = ?1 OR second_word_id = ?1
        ";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let rows = query.query(params![word_id]).unwrap();

        let mut vec: Vec<ExistingLinkedWord> = rows
            .map(|row| ExistingLinkedWord::try_from_row_populate_other(row, db, word_id))
            .collect()
            .unwrap();

        Span::current().record("results", &vec.len());

        vec.sort_by_key(|l| l.link_type);
        vec
    }

    #[instrument(name = "Fetch existing linked word", fields(found), skip(db))]
    pub fn fetch(
        db: &impl PublicAccessDb,
        id: u64,
        skip_populating: u64,
    ) -> Option<ExistingLinkedWord> {
        const SELECT: &str = "
            SELECT link_id, link_type, first_word_id, second_word_id FROM linked_words
                WHERE link_id = ?1;
        ";

        let conn = db.get().unwrap();
        let opt = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![id], |row| {
                ExistingLinkedWord::try_from_row_populate_other(row, db, skip_populating)
            })
            .optional()
            .unwrap();

        Span::current().record("found", &opt.is_some());

        opt
    }

    #[instrument(name = "Populate existing linked word", fields(link_id), skip(row, db))]
    pub fn try_from_row_populate_other(
        row: &Row<'_>,
        db: &impl PublicAccessDb,
        skip_populating: u64,
    ) -> Result<Self, rusqlite::Error> {
        let (first_word_id, second_word_id) =
            (row.get("first_word_id")?, row.get("second_word_id")?);
        let populate = if first_word_id != skip_populating {
            first_word_id
        } else {
            second_word_id
        };

        let link_id = row.get("link_id")?;

        Span::current().record("link_id", &link_id);

        Ok(ExistingLinkedWord {
            link_id,
            first_word_id,
            second_word_id,
            link_type: row.get("link_type")?,
            other: WordHit::fetch_from_db(db, WordOrSuggestionId::existing(populate)).unwrap(),
        })
    }
}
