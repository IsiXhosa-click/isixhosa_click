use warp::{Rejection, Filter, Reply};
use askama::Template;
use crate::language::{PartOfSpeech, NounClass, WordLinkType};
use std::fmt::{Display, Formatter};
use std::fmt;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::FromSql;
use r2d2_sqlite::rusqlite::types::{FromSqlResult, ValueRef};
use warp::path::param;
use fallible_iterator::FallibleIterator;
use rusqlite::{OptionalExtension, Row, params, Statement};
use serde::Deserialize;
use crate::submit::edit_suggestion;
use std::convert::{TryFrom, TryInto};
use crate::typesense::WordHit;
use crate::database::get_word_hit_from_db;

struct ExistingWord {
    word_id: i64,

    english: String,
    xhosa: String,
    part_of_speech: PartOfSpeech,

    xhosa_tone_markings: String,
    infinitive: String,
    is_plural: bool,
    noun_class: Option<NounClass>,
    note: String,

    examples: Vec<ExistingExample>,
    linked_words: Vec<ExistingLinkedWord>,
}

impl ExistingWord {
    // TODO(cleanup) use TryFrom
    fn try_from_row(row: &Row<'_>) -> Result<Self, rusqlite::Error> {
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
            linked_words: vec![]
        })
    }
}

struct ExistingExample {
    example_id: i64,
    word_or_suggested_id: WordOrSuggestedId,

    english: String,
    xhosa: String,
}

impl TryFrom<&Row<'_>> for ExistingExample {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(ExistingExample {
            example_id: row.get("example_id")?,
            english: row.get("english")?,
            xhosa: row.get("xhosa")?,
            word_or_suggested_id: row.try_into()?,
        })
    }
}

struct ExistingLinkedWord {
    link_id: i64,
    first_word_id: i64,
    second_word_id: i64,
    link_type: WordLinkType,
}

impl TryFrom<&Row<'_>> for ExistingLinkedWord {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(ExistingLinkedWord {
            link_id: row.get("link_id")?,
            first_word_id: row.get("first_word_id")?,
            second_word_id: row.get("second_word_id")?,
            link_type: row.get("link_type")?,
        })
    }
}


#[derive(Template)]
#[template(path = "accept.html")]
struct SuggestedWords {
    suggestions: Vec<SuggestedWord>
}

#[derive(Clone, Debug)]
pub struct SuggestedWord {
    pub suggestion_id: i64,
    pub word_id: Option<i64>,

    pub changes_summary: String,
    pub deletion: bool,

    pub english: MaybeEdited<String>,
    pub xhosa: MaybeEdited<String>,
    pub part_of_speech: MaybeEdited<PartOfSpeech>,

    pub xhosa_tone_markings: MaybeEdited<String>,
    pub infinitive: MaybeEdited<String>,
    pub is_plural: MaybeEdited<bool>,
    pub noun_class: MaybeEdited<Option<NounClass>>,
    pub note: MaybeEdited<String>,

    pub examples: Vec<SuggestedExample>,
    pub linked_words: Vec<SuggestedLinkedWord>,
}

impl SuggestedWord {
    fn from_row(row: &Row<'_>, select_original: &mut Statement<'_>) -> Self {
        let e = if let Some(word) = row.get::<&str, Option<i64>>("existing_word_id").unwrap() {
            Some(select_original
                .query_row(params![word], ExistingWord::try_from_row)
                .unwrap())
        } else {
            None
        };

        let e = e.as_ref();

        let noun_class = row.get::<&str, Option<NounClass>>("noun_class");
        let old_noun_class = e.and_then(|e| e.noun_class);
        let noun_class = match (noun_class, old_noun_class) {
            (Ok(None), old) => MaybeEdited::Old(old),
            (Ok(new @ Some(_)), old @ Some(_)) => MaybeEdited::Edited { old, new },
            (Ok(new @ Some(_)), None) => MaybeEdited::New(new),
            // Error is assumed to be discrim out of range (assumed to be 255) and this means deletion
            (Err(_), old @ Some(_)) => MaybeEdited::Edited { new: None, old },
            (Err(_), None) => MaybeEdited::Old(None),
        };

        SuggestedWord {
            suggestion_id: row.get("suggestion_id").unwrap(),
            word_id: row.get("existing_word_id").unwrap(),
            changes_summary: row.get("changes_summary").unwrap(),
            deletion: row.get("deletion").unwrap(),
            english: MaybeEdited::from_row("english", row, e.map(|e| e.english.clone())),
            xhosa: MaybeEdited::from_row("xhosa", row, e.map(|e| e.xhosa.clone())),
            part_of_speech: MaybeEdited::from_row("part_of_speech", row, e.map(|e| e.part_of_speech)),
            xhosa_tone_markings: MaybeEdited::from_row("xhosa_tone_markings", row, e.map(|e| e.xhosa_tone_markings.clone())),
            infinitive: MaybeEdited::from_row("infinitive", row, e.map(|e| e.infinitive.clone())),
            is_plural: MaybeEdited::from_row("is_plural", row, e.map(|e| e.is_plural)),
            noun_class,
            note: MaybeEdited::from_row("note", row, e.map(|e| e.note.clone())),
            examples: vec![],
            linked_words: vec![]
        }
    }
}

#[derive(Clone, Debug)]
pub struct SuggestedExample {
    pub deletion: bool,
    pub changes_summary: String,

    pub suggestion_id: i64,
    pub existing_example_id: Option<i64>,
    pub word_or_suggested_id: WordOrSuggestedId,

    pub english: MaybeEdited<String>,
    pub xhosa: MaybeEdited<String>,
}

impl SuggestedExample {
    fn from_row(row: &Row<'_>, select_original: &mut Statement<'_>) -> Self {
        let e = if let Some(example) = row.get::<&str, Option<i64>>("existing_example_id").unwrap() {
            Some(select_original
                .query_row(params![example], ExistingWord::try_from_row)
                .unwrap())
        } else {
            None
        };

        let e = e.as_ref();

        SuggestedExample {
            deletion: row.get("deletion").unwrap(),
            changes_summary: row.get("changes_summary").unwrap(),
            suggestion_id: row.get("suggestion_id").unwrap(),
            existing_example_id: row.get("existing_example_id").unwrap(),
            word_or_suggested_id: row.try_into().unwrap(),
            english: MaybeEdited::from_row("english", row, e.map(|e| e.english.clone())),
            xhosa: MaybeEdited::from_row("xhosa", row, e.map(|e| e.xhosa.clone())),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SuggestedLinkedWord {
    pub deletion: bool,
    pub changes_summary: String,
    pub suggestion_id: i64,

    pub first_existing_word_id: i64,
    pub second: WordOrSuggestedId,
    pub link_type: MaybeEdited<WordLinkType>,

    pub other: WordHit,
}

impl SuggestedLinkedWord {
    fn from_row_populate_other(
        row: &Row<'_>,
        db: Pool<SqliteConnectionManager>,
        select_original: &mut Statement<'_>
    ) -> Self {
        let e = if let Some(example) = row.get::<&str, Option<i64>>("existing_linked_word_id").unwrap() {
            Some(select_original
                .query_row(params![example], |row| ExistingLinkedWord::try_from(row))
                .unwrap())
        } else {
            None
        };

        let e = e.as_ref();

        let first_existing_word_id = row.get("first_existing_word_id").unwrap();
        let other = get_word_hit_from_db(db, first_existing_word_id).unwrap();

        SuggestedLinkedWord {
            deletion: row.get("deletion").unwrap(),
            changes_summary: row.get("changes_summary").unwrap(),
            suggestion_id: row.get("suggestion_id").unwrap(),

            first_existing_word_id,
            second: WordOrSuggestedId::try_from_row(row, "second_existing_word_id", "suggested_word_id").unwrap(),
            link_type: MaybeEdited::from_row("link_type", row, e.map(|e| e.link_type)),
            other,
        }
    }
}

#[derive(Clone, Debug)]
pub enum MaybeEdited<T> {
    Edited {
        old: T,
        new: T,
    },
    Old(T),
    New(T),
}

impl<T> MaybeEdited<T> {
    pub fn current(&self) -> &T {
        match self {
            MaybeEdited::Edited { new, .. } => new,
            MaybeEdited::Old(old) => old,
            MaybeEdited::New(new) => new,
        }
    }
}

impl MaybeEdited<String> {
    pub fn is_empty(&self) -> bool {
        match self {
            MaybeEdited::Edited { new, old } => new.is_empty() && old.is_empty(),
            MaybeEdited::Old(v) => v.is_empty(),
            MaybeEdited::New(v) => v.is_empty(),
        }
    }
}

impl<T: FromSql> MaybeEdited<T> {
    fn from_row(idx: &str, row: &Row<'_>, existing: Option<T>) -> MaybeEdited<T> {
        match (row.get::<&str, Option<T>>(idx).unwrap(), existing) {
            (Some(new), Some(old)) => MaybeEdited::Edited { old, new },
            (Some(new), None) => MaybeEdited::New(new),
            (None, Some(old)) => MaybeEdited::Old(old),
            (None, None) => panic!(
                "Field in suggestion unfilled; this is an error! Suggestion id: {:?}. Index: {}",
                row.get::<&str, i64>("suggestion_id"),
                idx,
            ),
        }
    }
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
        suggested_idx: &str
    ) -> Result<WordOrSuggestedId, rusqlite::Error> {
        let existing_word_id: Option<i64> = row.get(existing_idx).unwrap();
        let suggested_word_id: Option<i64> = row.get(suggested_idx).unwrap();
        match (existing_word_id, suggested_word_id) {
            (Some(existing), None) => Ok(WordOrSuggestedId::ExistingWord(existing)),
            (None, Some(suggested)) => Ok(WordOrSuggestedId::Suggested(suggested)),
            (existing, suggested) => {
                panic!(
                    "Invalid pair of exisitng/suggested ids: existing - {:?} suggested - {:?}",
                    existing,
                    suggested_word_id
                )
            }
        }
    }
}

impl TryFrom<&Row<'_>> for WordOrSuggestedId {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        WordOrSuggestedId::try_from_row(
            row,
            "existing_word_id",
            "suggested_word_id"
        )
    }
}

pub fn accept(
    db: Pool<SqliteConnectionManager>,
) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    #[derive(Deserialize)]
    struct Params {
        suggestion: i64,
    }

    let db = warp::any().map(move || db.clone());

    let show_all = warp::get()
        .and(db.clone())
        .and_then(suggested_words);

    let edit_one = warp::post()
        .and(db)
        .and(warp::body::form::<Params>())
        .and_then(|db, params: Params| edit_suggestion(db, params.suggestion));

    // TODO accept form submit too

    warp::path("accept").and(warp::path::end()).and(show_all.or(edit_one))
}

const SELECT_ORIGINAL: &str = "
    SELECT
        word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
        noun_class, note
    from words WHERE word_id = ?1;";

/// Returns the suggested word without examples and linked words populated.
pub fn get_suggested_word_alone(db: Pool<SqliteConnectionManager>, id: i64) -> Option<SuggestedWord> {
    const SELECT_SUGGESTION: &str = "SELECT
            suggestion_id, existing_word_id, changes_summary, deletion,
            english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            noun_class, note
        from word_suggestions WHERE suggestion_id=?1;";

    let conn = db.get().unwrap();

    let mut select_original = conn.prepare(SELECT_ORIGINAL).unwrap();

    // WTF rustc?
    let v = conn
        .prepare(SELECT_SUGGESTION)
        .unwrap()
        .query_row(params![id], |row| Ok(SuggestedWord::from_row(row, &mut select_original)))
        .optional()
        .unwrap();
    v
}

/// Returns the suggested word with examples and linked words populated.
pub fn get_full_suggested_word(db: Pool<SqliteConnectionManager>, id: i64) -> Option<SuggestedWord> {
    let mut word = get_suggested_word_alone(db.clone(), id);
    if let Some(word) = word.as_mut() {
        word.examples = get_examples_for_suggestion(db.clone(), id);
        word.linked_words = get_linked_words_for_suggestion(db, id);
    }

    word
}

pub fn get_examples_for_suggestion(db: Pool<SqliteConnectionManager>, suggested_word_id: i64) -> Vec<SuggestedExample> {
    const SELECT_SUGGESTION: &str = "
        SELECT suggestion_id, existing_word_id, suggested_word_id, existing_example_id, deletion, changes_summary, xhosa, english
            FROM example_suggestions WHERE suggested_word_id = ?1;";

    const SELECT_ORIGINAL: &str = "
        SELECT example_id, word_id, english, xhosa FROM examples WHERE example_id = ?1;";

    let conn = db.get().unwrap();
    let mut query = conn.prepare(SELECT_SUGGESTION).unwrap();
    let mut select_original = conn.prepare(SELECT_ORIGINAL).unwrap();
    let examples = query.query(params![suggested_word_id]).unwrap();

    examples.map(|row| Ok(SuggestedExample::from_row(row, &mut select_original))).collect().unwrap()
}

pub fn get_linked_words_for_suggestion(db: Pool<SqliteConnectionManager>, suggested_word_id: i64) -> Vec<SuggestedLinkedWord> {
    const SELECT_SUGGESTION: &str = "
        SELECT suggestion_id, link_type, deletion, changes_summary, existing_linked_word_id,
            first_existing_word_id, second_existing_word_id, suggested_word_id
            FROM linked_word_suggestions WHERE suggested_word_id = ?1;";

    const SELECT_ORIGINAL: &str = "
        SELECT link_id, link_type, first_word_id, second_word_id FROM linked_words WHERE link_id = ?1;";

    let conn = db.get().unwrap();
    let mut query = conn.prepare(SELECT_SUGGESTION).unwrap();
    let mut select_original = conn.prepare(SELECT_ORIGINAL).unwrap();
    let examples = query.query(params![suggested_word_id]).unwrap();

    examples.map(|row| Ok(SuggestedLinkedWord::from_row_populate_other(row, db.clone(),&mut select_original))).collect().unwrap()
}

async fn suggested_words(
    db: Pool<SqliteConnectionManager>,
) -> Result<impl warp::Reply, Rejection> {
    let suggestions = tokio::task::spawn_blocking(move || {
        const SELECT_SUGGESTIONS: &str = "SELECT
            suggestion_id, existing_word_id, changes_summary, deletion,
            english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            noun_class, note
        from word_suggestions;";

        let conn = db.get().unwrap();

        let mut query = conn.prepare(SELECT_SUGGESTIONS).unwrap();
        let suggestions = query.query(params![]).unwrap();
        let mut select_original = conn.prepare(SELECT_ORIGINAL).unwrap();

        suggestions.map(|row| {
            let mut word = SuggestedWord::from_row(row, &mut select_original);
            word.examples = get_examples_for_suggestion(db.clone(), word.suggestion_id);
            word.linked_words = get_linked_words_for_suggestion(db.clone(), word.suggestion_id);

            Ok(word)
        })
        .collect()
        .unwrap()
    }).await.unwrap();

    Ok(SuggestedWords { suggestions })
}
