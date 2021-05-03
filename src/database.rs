//! TODO(cleanup) refactor to put all DB stuff here or in a module under here

use std::convert::TryFrom;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{OptionalExtension, params, Row};

use crate::database::suggestion::{SuggestedExample, SuggestedLinkedWord, SuggestedWord};
use crate::language::SerializeDisplay;
use crate::typesense::WordHit;

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
                noun_class: row.get("noun_class").unwrap(),
            })
        })
        .optional()
        .unwrap();
    v
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

pub fn accept_new_word_suggestion(db: Pool<SqliteConnectionManager>, s: SuggestedWord) -> i64 {
    let word_suggestion_id = s.suggestion_id;
    let word_id = accept_word_suggestion(db.clone(), &s, false);

    for mut example in s.examples.into_iter() {
        example.word_or_suggested_id = WordOrSuggestedId::ExistingWord(word_id);
        accept_example(db.clone(), example);
    }

    for mut linked_word in s.linked_words.into_iter() {
        linked_word.second = WordOrSuggestedId::ExistingWord(word_id);
        accept_linked_word(db.clone(), linked_word);
    }

    SuggestedWord::delete(db, word_suggestion_id);

    word_id
}

pub fn accept_word_suggestion(db: Pool<SqliteConnectionManager>, s: &SuggestedWord, delete: bool) -> i64 {
    const INSERT: &str = "
        INSERT INTO words (
            word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            noun_class, note
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(word_id) DO UPDATE SET
                english = excluded.english,
                xhosa = excluded.xhosa,
                part_of_speech = excluded.part_of_speech,
                xhosa_tone_markings = excluded.xhosa_tone_markings,
                infinitive = excluded.infinitive,
                is_plural = excluded.is_plural,
                noun_class = excluded.noun_class,
                note = excluded.note
            RETURNING word_id;
    ";

    let conn = db.get().unwrap();
    let params = params![
        s.word_id, s.english.current(), s.xhosa.current(), s.part_of_speech.current(),
        s.xhosa_tone_markings.current(), s.infinitive.current(), s.is_plural.current(),
        s.noun_class.current(), s.note.current(),
    ];

    let id = conn
        .prepare(INSERT)
        .unwrap()
        .query_row(params, |row| row.get("word_id"))
        .unwrap();

    if delete {
        SuggestedWord::delete(db, s.suggestion_id);
    }

    id
}

pub fn accept_linked_word(
    db: Pool<SqliteConnectionManager>,
    s: SuggestedLinkedWord,
) -> i64 {
    const INSERT: &str = "
        INSERT INTO linked_words (link_id, link_type, first_word_id, second_word_id)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(link_id) DO UPDATE SET
                link_type = excluded.link_type
            RETURNING link_id;
    ";

    let conn = db.get().unwrap();
    let second_existing = match s.second {
        WordOrSuggestedId::ExistingWord(e) => e,
        _ => panic!("No existing word for suggested linked word {:#?}", s),
    };

    let params = params![
        s.existing_linked_word_id, s.link_type.current(), s.first_existing_word_id, second_existing
    ];

    let id = conn
        .prepare(INSERT)
        .unwrap()
        .query_row(params, |row| row.get("link_id"))
        .unwrap();

    SuggestedLinkedWord::delete(db, s.suggestion_id);

    id
}

pub fn accept_example(db: Pool<SqliteConnectionManager>, s: SuggestedExample) -> i64 {
    const INSERT: &str = "
        INSERT INTO examples (example_id, word_id, english, xhosa) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(example_id) DO UPDATE SET
                english = excluded.english,
                xhosa = excluded.xhosa
            RETURNING example_id;
    ";

    let conn = db.get().unwrap();
    let word = match s.word_or_suggested_id {
        WordOrSuggestedId::ExistingWord(e) => e,
        _ => panic!("No existing word for suggested example {:#?}", s),
    };
    let params = params![
        s.existing_example_id, word, s.english.current(), s.xhosa.current()
    ];

    let id = conn
        .prepare(INSERT)
        .unwrap()
        .query_row(params, |row| row.get("example_id"))
        .unwrap();

    SuggestedExample::delete(db, s.suggestion_id);

    id
}
