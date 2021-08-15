//! TODO(cleanup) refactor to put all DB stuff here or in a module under here

use std::convert::TryFrom;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension, Row};
use serde::Deserialize;

use crate::database::suggestion::{
    MaybeEdited, SuggestedExample, SuggestedLinkedWord, SuggestedWord,
};
use crate::language::{NounClassOpt, NounClassOptExt, SerializeDisplay};
use crate::search::WordHit;

pub mod deletion;
pub mod existing;
pub mod suggestion;

// TODO this assumes unedited suggestion
pub fn get_word_hit_from_db(
    db: &Pool<SqliteConnectionManager>,
    id: WordOrSuggestionId,
) -> Option<WordHit> {
    const SELECT_EXISTING: &str =
        "SELECT english, xhosa, part_of_speech, is_plural, noun_class FROM words
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
    let v = conn
        .prepare(stmt)
        .unwrap()
        .query_row(params![id.inner()], |row| {
            Ok(WordHit {
                id: id.inner() as u64,
                english: row.get("english").unwrap(),
                xhosa: row.get("xhosa").unwrap(),
                part_of_speech: SerializeDisplay(row.get("part_of_speech").unwrap()),
                is_plural: row.get("is_plural").unwrap(),
                noun_class: row
                    .get::<&str, Option<NounClassOpt>>("noun_class")
                    .unwrap()
                    .flatten(),
            })
        })
        .optional()
        .unwrap();
    v
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum WordOrSuggestionId {
    ExistingWord { existing_id: u64 },
    Suggested { suggestion_id: u64 },
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

// TODO move to suggestedword
pub fn accept_whole_word_suggestion(db: &Pool<SqliteConnectionManager>, s: SuggestedWord) -> u64 {
    let word_suggestion_id = s.suggestion_id;
    let word_id = accept_just_word_suggestion(&db, &s, false);

    for mut example in s.examples.into_iter() {
        example.word_or_suggested_id = WordOrSuggestionId::ExistingWord {
            existing_id: word_id,
        };
        accept_example(&db, example);
    }

    for mut linked_word in s.linked_words.into_iter() {
        linked_word.second = MaybeEdited::New((
            WordOrSuggestionId::ExistingWord {
                existing_id: word_id,
            },
            WordHit::empty(),
        ));
        accept_linked_word(&db, linked_word);
    }

    SuggestedWord::delete(db, word_suggestion_id);

    word_id
}

pub fn accept_just_word_suggestion(
    db: &Pool<SqliteConnectionManager>,
    s: &SuggestedWord,
    delete: bool,
) -> u64 {
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
        s.word_id,
        s.english.current(),
        s.xhosa.current(),
        s.part_of_speech.current(),
        s.xhosa_tone_markings.current(),
        s.infinitive.current(),
        s.is_plural.current(),
        s.noun_class.current(),
        s.note.current(),
    ];

    let id: i64 = conn
        .prepare(INSERT)
        .unwrap()
        .query_row(params, |row| row.get("word_id"))
        .unwrap();

    if delete {
        SuggestedWord::delete(db, s.suggestion_id);
    }

    id as u64
}

pub fn accept_linked_word(db: &Pool<SqliteConnectionManager>, s: SuggestedLinkedWord) -> i64 {
    const INSERT: &str = "
        INSERT INTO linked_words (link_id, link_type, first_word_id, second_word_id)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(link_id) DO UPDATE SET
                link_type = excluded.link_type
            RETURNING link_id;
    ";

    let conn = db.get().unwrap();
    let second_existing = match s.second.current().0 {
        WordOrSuggestionId::ExistingWord { existing_id } => existing_id,
        _ => panic!("No existing word for suggested linked word {:#?}", s),
    };

    let params = params![
        s.existing_linked_word_id,
        s.link_type.current(),
        s.first.current().0,
        second_existing
    ];

    let id = conn
        .prepare(INSERT)
        .unwrap()
        .query_row(params, |row| row.get("link_id"))
        .unwrap();

    SuggestedLinkedWord::delete(db, s.suggestion_id);

    id
}

pub fn accept_example(db: &Pool<SqliteConnectionManager>, s: SuggestedExample) -> i64 {
    const INSERT: &str = "
        INSERT INTO examples (example_id, word_id, english, xhosa) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(example_id) DO UPDATE SET
                english = excluded.english,
                xhosa = excluded.xhosa
            RETURNING example_id;
    ";

    let conn = db.get().unwrap();
    let word = match s.word_or_suggested_id {
        WordOrSuggestionId::ExistingWord { existing_id } => existing_id,
        _ => panic!("No existing word for suggested example {:#?}", s),
    };
    let params = params![
        s.existing_example_id,
        word,
        s.english.current(),
        s.xhosa.current()
    ];

    let id = conn
        .prepare(INSERT)
        .unwrap()
        .query_row(params, |row| row.get("example_id"))
        .unwrap();

    SuggestedExample::delete(db, s.suggestion_id);

    id
}
