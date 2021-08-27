use crate::database::existing::ExistingExample;
use crate::database::existing::ExistingWord;
use crate::database::WordOrSuggestionId;
use crate::language::{PartOfSpeech, WordLinkType};
use crate::search::WordHit;
use crate::serialization::NounClassOpt;
use fallible_iterator::FallibleIterator;
use isixhosa::noun::NounClass;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::FromSql;
use rusqlite::{params, OptionalExtension, Row};
use std::collections::HashMap;
use std::convert::TryInto;
use crate::submit::WordId;

#[derive(Clone, Debug)]
pub struct SuggestedWord {
    pub suggestion_id: u64,
    pub word_id: Option<u64>,

    pub changes_summary: String,

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
    pub fn this_id(&self) -> WordOrSuggestionId {
        if let Some(word_id) = self.word_id {
            WordOrSuggestionId::ExistingWord {
                existing_id: word_id,
            }
        } else {
            WordOrSuggestionId::Suggested {
                suggestion_id: self.suggestion_id,
            }
        }
    }

    pub fn fetch_all_full(db: &Pool<SqliteConnectionManager>) -> Vec<SuggestedWord> {
        const SELECT_SUGGESTIONS: &str = "
            SELECT
                suggestion_id, existing_word_id, changes_summary,
                english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
                noun_class, note
            FROM word_suggestions
            ORDER BY suggestion_id;";

        let conn = db.get().unwrap();

        let mut query = conn.prepare(SELECT_SUGGESTIONS).unwrap();
        let suggestions = query.query(params![]).unwrap();

        suggestions
            .map(|row| {
                let mut w = SuggestedWord::from_row_fetch_original(row, &db);
                w.examples = SuggestedExample::fetch_all_for_suggestion(&db, w.suggestion_id);
                w.linked_words =
                    SuggestedLinkedWord::fetch_all_for_suggestion(&db, w.suggestion_id);

                Ok(w)
            })
            .collect()
            .unwrap()
    }

    /// Returns the suggested word without examples and linked words populated.
    pub fn fetch_alone(db: &Pool<SqliteConnectionManager>, id: u64) -> Option<SuggestedWord> {
        const SELECT_SUGGESTION: &str = "SELECT
            suggestion_id, existing_word_id, changes_summary,
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
    pub fn fetch_full(db: &Pool<SqliteConnectionManager>, id: u64) -> Option<SuggestedWord> {
        let mut word = SuggestedWord::fetch_alone(&db, id);
        if let Some(w) = word.as_mut() {
            w.examples = SuggestedExample::fetch_all_for_suggestion(&db, id);
            w.linked_words = SuggestedLinkedWord::fetch_all_for_suggestion(&db, id);
        }

        word
    }

    pub fn delete(db: &Pool<SqliteConnectionManager>, id: u64) -> bool {
        const DELETE: &str = "DELETE FROM word_suggestions WHERE suggestion_id = ?1";

        let conn = db.get().unwrap();
        let modified_rows = conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
        modified_rows == 1
    }

    fn from_row_fetch_original(row: &Row<'_>, db: &Pool<SqliteConnectionManager>) -> Self {
        let existing_id = row.get::<&str, Option<i64>>("existing_word_id").unwrap();
        let e = existing_id.and_then(|id| ExistingWord::fetch_alone(db, id as u64));
        let e = e.as_ref();

        let noun_class = row.get::<&str, Option<NounClassOpt>>("noun_class");
        let old = e.and_then(|e| e.noun_class);
        let noun_class = match noun_class {
            Ok(Some(NounClassOpt::Remove)) if old != None => MaybeEdited::Edited { old, new: None },
            Ok(Some(NounClassOpt::Remove)) => MaybeEdited::Old(None),
            Ok(Some(NounClassOpt::Some(new))) => MaybeEdited::Edited {
                old,
                new: Some(new),
            },
            Ok(None) => MaybeEdited::Old(old),
            Err(e) => panic!("Invalid noun class discriminator in database: {:?}", e),
        };

        SuggestedWord {
            suggestion_id: row.get("suggestion_id").unwrap(),
            word_id: row.get("existing_word_id").unwrap(),
            changes_summary: row.get("changes_summary").unwrap(),
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

    pub fn fetch_existing_id_for_suggestion(
        db: &Pool<SqliteConnectionManager>,
        suggestion: u64,
    ) -> Option<u64> {
        const SELECT: &str =
            "SELECT existing_word_id FROM word_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        let word_id = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![suggestion], |row| row.get("existing_word_id"))
            .unwrap();
        word_id
    }
}

#[derive(Clone, Debug)]
pub struct SuggestedExample {
    pub changes_summary: String,

    pub suggestion_id: u64,
    pub existing_example_id: Option<u64>,
    pub word_or_suggested_id: WordOrSuggestionId,

    pub english: MaybeEdited<String>,
    pub xhosa: MaybeEdited<String>,
}

impl SuggestedExample {
    pub fn fetch_all_for_existing_words(
        db: &Pool<SqliteConnectionManager>,
    ) -> impl Iterator<Item = (WordId, Vec<SuggestedExample>)> {
        const SELECT: &str = "
            SELECT words.word_id,
                   example_suggestions.suggestion_id, example_suggestions.existing_word_id,
                   example_suggestions.existing_example_id, example_suggestions.changes_summary,
                   example_suggestions.xhosa, example_suggestions.suggested_word_id,
                   example_suggestions.english
            FROM example_suggestions
            INNER JOIN words
            ON example_suggestions.existing_word_id = words.word_id;
        ";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let examples = query.query(params![]).unwrap();

        let mut map: HashMap<WordId, Vec<SuggestedExample>> = HashMap::new();

        examples
            .map(|row| {
                Ok((
                    WordId(row.get::<&str, u64>("word_id")?),
                    SuggestedExample::from_row_fetch_original(&row, &db),
                ))
            })
            .for_each(|(word_id, example)| {
                map.entry(word_id)
                    .or_insert_with(|| Vec::with_capacity(1))
                    .push(example);
                Ok(())
            })
            .unwrap();

        map.into_iter()
    }

    pub fn fetch_all_for_suggestion(
        db: &Pool<SqliteConnectionManager>,
        suggested_word_id: u64,
    ) -> Vec<SuggestedExample> {
        const SELECT_SUGGESTION: &str = "
        SELECT suggestion_id, existing_word_id, suggested_word_id, existing_example_id, changes_summary, xhosa, english
            FROM example_suggestions WHERE suggested_word_id = ?1;";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT_SUGGESTION).unwrap();
        let examples = query.query(params![suggested_word_id]).unwrap();

        examples
            .map(|row| Ok(SuggestedExample::from_row_fetch_original(row, &db)))
            .collect()
            .unwrap()
    }

    pub fn fetch(
        db: &Pool<SqliteConnectionManager>,
        suggestion_id: u64,
    ) -> Option<SuggestedExample> {
        const SELECT: &str = "
            SELECT suggestion_id, existing_word_id, suggested_word_id, existing_example_id,
                   changes_summary, xhosa, english
            FROM example_suggestions WHERE suggestion_id = ?1";

        let conn = db.get().unwrap();
        let ex = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![suggestion_id], |row| {
                Ok(Self::from_row_fetch_original(row, db))
            })
            .optional()
            .unwrap();
        ex
    }

    pub fn delete(db: &Pool<SqliteConnectionManager>, id: u64) -> bool {
        const DELETE: &str = "DELETE FROM example_suggestions WHERE suggestion_id = ?1";

        let conn = db.get().unwrap();
        let modified_rows = conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
        modified_rows == 1
    }

    fn from_row_fetch_original(row: &Row<'_>, db: &Pool<SqliteConnectionManager>) -> Self {
        let existing_id = row.get::<&str, Option<i64>>("existing_example_id").unwrap();
        let e = existing_id.and_then(|id| ExistingExample::get(db, id as u64));
        let e = e.as_ref();

        SuggestedExample {
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
    pub changes_summary: String,
    pub suggestion_id: u64,
    pub existing_linked_word_id: Option<u64>,

    pub first: MaybeEdited<(u64, WordHit)>,
    pub second: MaybeEdited<(WordOrSuggestionId, WordHit)>,
    pub link_type: MaybeEdited<WordLinkType>,
}

impl SuggestedLinkedWord {
    pub fn fetch_all_for_suggestion(
        db: &Pool<SqliteConnectionManager>,
        suggested_word_id: u64,
    ) -> Vec<SuggestedLinkedWord> {
        const SELECT_SUGGESTION: &str = "
        SELECT suggestion_id, link_type, changes_summary, existing_linked_word_id,
            first_existing_word_id, second_existing_word_id, suggested_word_id
            FROM linked_word_suggestions WHERE suggested_word_id = ?1;";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT_SUGGESTION).unwrap();
        let rows = query.query(params![suggested_word_id]).unwrap();

        let mut vec: Vec<SuggestedLinkedWord> = rows
            .map(|row| Ok(SuggestedLinkedWord::from_row_populate_both(row, db)))
            .collect()
            .unwrap();

        vec.sort_by_key(|link| *link.link_type.current());

        vec
    }

    pub fn delete(db: &Pool<SqliteConnectionManager>, id: u64) -> bool {
        const DELETE: &str = "DELETE FROM linked_word_suggestions WHERE suggestion_id = ?1";

        let conn = db.get().unwrap();
        let modified_rows = conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
        modified_rows == 1
    }

    fn from_row_populate_both(row: &Row<'_>, db: &Pool<SqliteConnectionManager>) -> Self {
        let conn = db.get().unwrap();
        let existing_id = row
            .get::<&str, Option<i64>>("existing_linked_word_id")
            .unwrap();
        let (other_type, other_first, other_second) = if let Some(id) = existing_id {
            let trio = conn
                .prepare("SELECT link_id FROM linked_words WHERE link_id = ?1")
                .unwrap()
                .query_row(params![id], |r| {
                    Ok((
                        r.get("link_id")?,
                        r.get("first_word_id")?,
                        r.get("second_word_id")?,
                    ))
                })
                .unwrap();
            (Some(trio.0), Some(trio.1), Some(trio.2))
        } else {
            (None, None, None)
        };

        let first_existing_word_id = row.get("first_existing_word_id").unwrap();
        let first_hit = WordHit::fetch_from_db(
            db,
            WordOrSuggestionId::ExistingWord {
                existing_id: first_existing_word_id,
            },
        )
        .unwrap();

        let first = if let Some(other_first) = other_first {
            let other_hit = WordHit::fetch_from_db(
                db,
                WordOrSuggestionId::ExistingWord {
                    existing_id: other_first,
                },
            )
            .unwrap();
            MaybeEdited::Edited {
                new: (first_existing_word_id, first_hit),
                old: (other_first, other_hit),
            }
        } else {
            MaybeEdited::New((first_existing_word_id, first_hit))
        };

        let second =
            WordOrSuggestionId::try_from_row(row, "second_existing_word_id", "suggested_word_id")
                .unwrap();
        let second_hit = WordHit::fetch_from_db(db, second).unwrap();

        let second = if let Some(other_second) = other_second {
            let other_hit = WordHit::fetch_from_db(
                db,
                WordOrSuggestionId::ExistingWord {
                    existing_id: other_second,
                },
            )
            .unwrap();
            MaybeEdited::Edited {
                new: (second, second_hit),
                old: (
                    WordOrSuggestionId::ExistingWord {
                        existing_id: other_second,
                    },
                    other_hit,
                ),
            }
        } else {
            MaybeEdited::New((second, second_hit))
        };

        SuggestedLinkedWord {
            changes_summary: row.get("changes_summary").unwrap(),
            suggestion_id: row.get("suggestion_id").unwrap(),

            existing_linked_word_id: row.get("existing_linked_word_id").unwrap(),
            first,
            second,
            link_type: MaybeEdited::from_row("link_type", row, other_type),
        }
    }

    pub fn other(&self, this_id: WordOrSuggestionId) -> MaybeEdited<WordHit> {
        if this_id == self.second.current().0 {
            self.first.map(|pair| pair.1.clone())
        } else {
            self.second.map(|pair| pair.1.clone())
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
    fn map<U, F: Fn(&T) -> U>(&self, f: F) -> MaybeEdited<U> {
        match self {
            MaybeEdited::Edited { new, old } => MaybeEdited::Edited {
                new: f(new),
                old: f(old),
            },
            MaybeEdited::Old(old) => MaybeEdited::Old(f(old)),
            MaybeEdited::New(new) => MaybeEdited::New(f(new)),
        }
    }

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
