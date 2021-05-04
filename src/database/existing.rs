use std::convert::{TryFrom, TryInto};

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{OptionalExtension, params, Row};

use crate::database::suggestion::{SuggestedExample, SuggestedLinkedWord, SuggestedWord};
use crate::database::{WordOrSuggestedId, get_word_hit_from_db};
use crate::language::{NounClass, PartOfSpeech, WordLinkType};
use fallible_iterator::FallibleIterator;
use crate::typesense::WordHit;

pub struct ExistingWord {
    pub word_id: i64,

    pub english: String,
    pub xhosa: String,
    pub part_of_speech: PartOfSpeech,

    pub xhosa_tone_markings: String,
    pub infinitive: String,
    pub is_plural: bool,
    pub noun_class: Option<NounClass>,
    pub note: String,

    pub examples: Vec<ExistingExample>,
    pub linked_words: Vec<ExistingLinkedWord>,
}

impl ExistingWord {
    pub fn get_full(db: Pool<SqliteConnectionManager>, id: i64) -> Option<ExistingWord> {
        let mut word = ExistingWord::get_alone(db.clone(), id);
        if let Some(word) = word.as_mut() {
            word.examples = ExistingExample::get_all_for_word(db.clone(), id);
            word.linked_words = ExistingLinkedWord::get_all_for_word(db, id);
        }

        word
    }

    pub fn get_alone(db: Pool<SqliteConnectionManager>, id: i64) -> Option<ExistingWord> {
        const SELECT_ORIGINAL: &str = "
        SELECT
            word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            noun_class, note
        from words WHERE word_id = ?1;";

        let conn = db.get().unwrap();
        let opt =conn.prepare(SELECT_ORIGINAL).unwrap()
            .query_row(params![id], |row| ExistingWord::try_from(row))
            .optional()
            .unwrap();
        opt
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
            noun_class: row.get("noun_class")?,
            note: row.get("note")?,
            examples: vec![],
            linked_words: vec![],
        })
    }
}

pub struct ExistingExample {
    pub example_id: i64,
    pub word_id: i64,

    pub english: String,
    pub xhosa: String,
}

impl ExistingExample {
    pub fn get_all_for_word(db: Pool<SqliteConnectionManager>, word_id: i64) -> Vec<ExistingExample> {
        const SELECT: &str = "SELECT example_id, word_id, english, xhosa FROM examples WHERE word_id = ?1";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let rows = query.query(params![word_id]).unwrap();

        rows
            .map(|row| ExistingExample::try_from(row))
            .collect()
            .unwrap()
    }

    pub fn get(db: Pool<SqliteConnectionManager>, example_id: i64) -> Option<ExistingExample> {
        const SELECT: &str = "SELECT example_id, word_id, english, xhosa FROM examples WHERE example_id = ?1";

        let conn = db.get().unwrap();
        let opt = conn.prepare(SELECT)
            .unwrap()
            .query_row(params![example_id], |row| ExistingExample::try_from(row))
            .optional()
            .unwrap();
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

pub struct ExistingLinkedWord {
    pub link_id: i64,
    pub first_word_id: i64,
    pub second_word_id: i64,
    pub link_type: WordLinkType,
    pub other: WordHit,
}

impl ExistingLinkedWord {
    pub fn get_all_for_word(db: Pool<SqliteConnectionManager>, word_id: i64) -> Vec<ExistingLinkedWord> {
        const SELECT: &str = "
            SELECT link_id, link_type, first_word_id, second_word_id FROM linked_words
                WHERE first_word_id = ?1 OR second_word_id = ?1
        ";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let rows = query.query(params![word_id]).unwrap();

        let mut vec: Vec<ExistingLinkedWord> = rows
            .map(|row| ExistingLinkedWord::try_from_row_populate_other(row, db.clone(), word_id))
            .collect()
            .unwrap();

        vec.sort_by_key(|l| l.link_type);

        vec
    }

    pub fn get(db: Pool<SqliteConnectionManager>, id: i64, skip_populating: i64) -> Option<ExistingLinkedWord> {
        const SELECT: &str = "
            SELECT link_id, link_type, first_word_id, second_word_id FROM linked_words
                WHERE link_id = ?1;
        ";

        let conn = db.get().unwrap();
        let opt = conn.prepare(SELECT)
            .unwrap()
            .query_row(params![id], |row| ExistingLinkedWord::try_from_row_populate_other(row, db.clone(), skip_populating))
            .optional()
            .unwrap();
        opt
    }

    fn try_from_row_populate_other(row: &Row<'_>, db: Pool<SqliteConnectionManager>, skip_populating: i64) -> Result<Self, rusqlite::Error> {
        let (first_word_id, second_word_id) = (row.get("first_word_id")?, row.get("second_word_id")?);
        let populate = if first_word_id != skip_populating {
            first_word_id
        } else {
            second_word_id
        };

        Ok(ExistingLinkedWord {
            link_id: row.get("link_id")?,
            first_word_id,
            second_word_id,
            link_type: row.get("link_type")?,
            other: get_word_hit_from_db(db, populate).unwrap(),
        })
    }
}
