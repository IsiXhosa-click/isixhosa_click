use crate::database::existing::ExistingExample;
use crate::database::existing::ExistingWord;
use crate::database::{get_word_hit_from_db, WordOrSuggestedId};
use crate::language::{NounClass, PartOfSpeech, WordLinkType};
use crate::typesense::WordHit;
use fallible_iterator::FallibleIterator;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::FromSql;
use rusqlite::{params, OptionalExtension, Row};
use std::convert::TryInto;

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
    pub fn get_all_full(db: Pool<SqliteConnectionManager>) -> Vec<SuggestedWord> {
        const SELECT_SUGGESTIONS: &str = "SELECT
            suggestion_id, existing_word_id, changes_summary, deletion,
            english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            noun_class, note
        from word_suggestions;";

        let conn = db.get().unwrap();

        let mut query = conn.prepare(SELECT_SUGGESTIONS).unwrap();
        let suggestions = query.query(params![]).unwrap();

        suggestions
            .map(|row| {
                let mut word = SuggestedWord::from_row_fetch_original(row, db.clone());
                word.examples =
                    SuggestedExample::get_all_for_suggestion(db.clone(), word.suggestion_id);
                word.linked_words =
                    SuggestedLinkedWord::get_all_for_suggestion(db.clone(), word.suggestion_id);

                Ok(word)
            })
            .collect()
            .unwrap()
    }

    /// Returns the suggested word without examples and linked words populated.
    pub fn get_alone(db: Pool<SqliteConnectionManager>, id: i64) -> Option<SuggestedWord> {
        const SELECT_SUGGESTION: &str = "SELECT
            suggestion_id, existing_word_id, changes_summary, deletion,
            english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            noun_class, note
        from word_suggestions WHERE suggestion_id=?1;";

        let conn = db.get().unwrap();

        // WTF rustc?
        let v = conn
            .prepare(SELECT_SUGGESTION)
            .unwrap()
            .query_row(params![id], |row| {
                Ok(SuggestedWord::from_row_fetch_original(row, db))
            })
            .optional()
            .unwrap();
        v
    }

    /// Returns the suggested word with examples and linked words populated.
    pub fn get_full(db: Pool<SqliteConnectionManager>, id: i64) -> Option<SuggestedWord> {
        let mut word = SuggestedWord::get_alone(db.clone(), id);
        if let Some(word) = word.as_mut() {
            word.examples = SuggestedExample::get_all_for_suggestion(db.clone(), id);
            word.linked_words = SuggestedLinkedWord::get_all_for_suggestion(db, id);
        }

        word
    }

    pub fn delete(db: Pool<SqliteConnectionManager>, id: i64) -> bool {
        const DELETE: &str = "DELETE FROM word_suggestions WHERE suggestion_id = ?1";

        let conn = db.get().unwrap();
        let modified_rows = conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
        modified_rows == 1
    }

    fn from_row_fetch_original(row: &Row<'_>, db: Pool<SqliteConnectionManager>) -> Self {
        let existing_id = row.get::<&str, Option<i64>>("existing_word_id").unwrap();
        let e = existing_id.and_then(|id| ExistingWord::get_alone(db, id));
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
            part_of_speech: MaybeEdited::from_row(
                "part_of_speech",
                row,
                e.map(|e| e.part_of_speech),
            ),
            xhosa_tone_markings: MaybeEdited::from_row(
                "xhosa_tone_markings",
                row,
                e.map(|e| e.xhosa_tone_markings.clone()),
            ),
            infinitive: MaybeEdited::from_row("infinitive", row, e.map(|e| e.infinitive.clone())),
            is_plural: MaybeEdited::from_row("is_plural", row, e.map(|e| e.is_plural)),
            noun_class,
            note: MaybeEdited::from_row("note", row, e.map(|e| e.note.clone())),
            examples: vec![],
            linked_words: vec![],
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
    pub fn get_all_for_suggestion(
        db: Pool<SqliteConnectionManager>,
        suggested_word_id: i64,
    ) -> Vec<SuggestedExample> {
        const SELECT_SUGGESTION: &str = "
        SELECT suggestion_id, existing_word_id, suggested_word_id, existing_example_id, deletion, changes_summary, xhosa, english
            FROM example_suggestions WHERE suggested_word_id = ?1;";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT_SUGGESTION).unwrap();
        let examples = query.query(params![suggested_word_id]).unwrap();

        examples
            .map(|row| Ok(SuggestedExample::from_row_fetch_original(row, db.clone())))
            .collect()
            .unwrap()
    }

    pub fn delete(db: Pool<SqliteConnectionManager>, id: i64) {
        const DELETE: &str = "DELETE FROM example_suggestions WHERE suggestion_id = ?1";

        let conn = db.get().unwrap();
        conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
    }

    fn from_row_fetch_original(row: &Row<'_>, db: Pool<SqliteConnectionManager>) -> Self {
        let existing_id = row.get::<&str, Option<i64>>("existing_example_id").unwrap();
        let e = existing_id.and_then(|id| ExistingExample::get(db, id));
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
    pub existing_linked_word_id: Option<i64>,

    pub first_existing_word_id: i64,
    pub second: WordOrSuggestedId,
    pub link_type: MaybeEdited<WordLinkType>,

    pub other: WordHit,
}

impl SuggestedLinkedWord {
    pub fn get_all_for_suggestion(
        db: Pool<SqliteConnectionManager>,
        suggested_word_id: i64,
    ) -> Vec<SuggestedLinkedWord> {
        const SELECT_SUGGESTION: &str = "
        SELECT suggestion_id, link_type, deletion, changes_summary, existing_linked_word_id,
            first_existing_word_id, second_existing_word_id, suggested_word_id
            FROM linked_word_suggestions WHERE suggested_word_id = ?1;";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT_SUGGESTION).unwrap();
        let rows = query.query(params![suggested_word_id]).unwrap();

        let mut vec: Vec<SuggestedLinkedWord> = rows
            .map(|row| {
                Ok(SuggestedLinkedWord::from_row_populate_other(
                    row,
                    db.clone(),
                ))
            })
            .collect()
            .unwrap();

        vec.sort_by_key(|link| *link.link_type.current());

        vec
    }

    pub fn delete(db: Pool<SqliteConnectionManager>, id: i64) {
        const DELETE: &str = "DELETE FROM linked_word_suggestions WHERE suggestion_id = ?1";

        let conn = db.get().unwrap();
        conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
    }

    fn from_row_populate_other(row: &Row<'_>, db: Pool<SqliteConnectionManager>) -> Self {
        let conn = db.get().unwrap();
        let existing_id = row
            .get::<&str, Option<i64>>("existing_linked_word_id")
            .unwrap();
        let other_type = existing_id.and_then(|id| {
            conn.prepare("SELECT link_id FROM linked_words WHERE link_id = ?1")
                .unwrap()
                .query_row(params![id], |row| row.get("link_id"))
                .optional()
                .unwrap()
        });

        let first_existing_word_id = row.get("first_existing_word_id").unwrap();
        let other = get_word_hit_from_db(db, first_existing_word_id).unwrap();

        SuggestedLinkedWord {
            deletion: row.get("deletion").unwrap(),
            changes_summary: row.get("changes_summary").unwrap(),
            suggestion_id: row.get("suggestion_id").unwrap(),

            existing_linked_word_id: row.get("existing_linked_word_id").unwrap(),
            first_existing_word_id,
            second: WordOrSuggestedId::try_from_row(
                row,
                "second_existing_word_id",
                "suggested_word_id",
            )
            .unwrap(),
            link_type: MaybeEdited::from_row("link_type", row, other_type),
            other,
        }
    }
}

#[derive(Clone, Debug)]
pub enum MaybeEdited<T> {
    Edited { old: T, new: T },
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

    pub fn old(&self) -> &T {
        match self {
            MaybeEdited::Edited { old, .. } => old,
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
