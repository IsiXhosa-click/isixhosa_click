use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Debug};
use std::sync::Arc;
use fallible_iterator::FallibleIterator;
use futures::executor::block_on;
use isixhosa::noun::NounClass;
use num_enum::TryFromPrimitive;
use rusqlite::{OptionalExtension, params, Row};
use rusqlite::types::FromSql;
use tracing::{instrument, Span};
use isixhosa_common::database::{ModeratorAccessDb, UserAccessDb};
use isixhosa_common::format::{DisplayHtml, HtmlFormatter, HyperlinkWrapper, NounClassInHit};
use isixhosa_common::language::{ConjunctionFollowedBy, PartOfSpeech, Transitivity, WordLinkType};
use isixhosa_common::serialization::WithDeleteSentinel;
use isixhosa_common::types::{ExistingExample, ExistingWord, PublicUserInfo, WordHit};
use crate::database::{add_attribution, WordOrSuggestionId};
use crate::database::WordId;
use crate::DebugExt;
use crate::search::{TantivyClient, WordDocument};

#[derive(Clone, Debug)]
pub struct SuggestedWord {
    pub suggestion_id: u64,
    pub suggesting_user: PublicUserInfo,
    pub word_id: Option<u64>,

    pub changes_summary: String,

    pub english: MaybeEdited<String>,
    pub xhosa: MaybeEdited<String>,
    pub part_of_speech: MaybeEdited<PartOfSpeech>,

    pub xhosa_tone_markings: MaybeEdited<String>,
    pub infinitive: MaybeEdited<String>,
    pub is_plural: MaybeEdited<bool>,
    pub is_inchoative: MaybeEdited<bool>,
    pub transitivity: MaybeEdited<Option<Transitivity>>,
    pub followed_by: MaybeEdited<Option<ConjunctionFollowedBy>>,
    pub noun_class: MaybeEdited<Option<NounClass>>,
    pub note: MaybeEdited<String>,

    pub is_informal: MaybeEdited<bool>,

    pub examples: Vec<SuggestedExample>,
    pub linked_words: Vec<SuggestedLinkedWord>,
}

impl SuggestedWord {
    pub fn this_id(&self) -> WordOrSuggestionId {
        if let Some(word_id) = self.word_id {
            WordOrSuggestionId::existing(word_id)
        } else {
            WordOrSuggestionId::suggested(self.suggestion_id)
        }
    }

    #[instrument(
        level = "info",
        name = "Fetch all suggested words",
        fields(results),
        skip(db)
    )]
    pub fn fetch_all_full(db: &impl ModeratorAccessDb) -> Vec<SuggestedWord> {
        const SELECT_SUGGESTIONS: &str = "
            SELECT
                suggestion_id, suggesting_user, existing_word_id, changes_summary,
                english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
                is_inchoative, is_informal, transitivity, followed_by, noun_class, note, username, display_name
            FROM word_suggestions
            INNER JOIN users ON word_suggestions.suggesting_user = users.user_id
            ORDER BY suggestion_id;";

        let conn = db.get().unwrap();

        let mut query = conn.prepare(SELECT_SUGGESTIONS).unwrap();
        let suggestions = query.query(params![]).unwrap();

        let results: Vec<_> = suggestions
            .map(|row| {
                let mut w = SuggestedWord::from_row_fetch_original(row, db);
                w.examples = SuggestedExample::fetch_all_for_suggestion(db, w.suggestion_id);
                w.linked_words = SuggestedLinkedWord::fetch_all_for_suggestion(db, w.suggestion_id);

                Ok(w)
            })
            .collect()
            .unwrap();

        Span::current().record("results", &results.len());

        results
    }

    /// Returns the suggested word without examples and linked words populated.
    #[instrument(
        level = "trace",
        name = "Fetch just suggested word",
        fields(found),
        skip(db)
    )]
    pub fn fetch_alone(db: &impl UserAccessDb, id: u64) -> Option<SuggestedWord> {
        const SELECT_SUGGESTION: &str = "
            SELECT
                suggestion_id, existing_word_id, changes_summary, english, xhosa, part_of_speech,
                xhosa_tone_markings, infinitive, is_plural, is_inchoative, is_informal, transitivity,
                followed_by, noun_class, note, username, display_name, suggesting_user
            FROM word_suggestions
            INNER JOIN users ON word_suggestions.suggesting_user = users.user_id
            WHERE suggestion_id = ?1;
        ";

        let conn = db.get().unwrap();

        let word = conn
            .prepare(SELECT_SUGGESTION)
            .unwrap()
            .query_row(params![id], |row| {
                Ok(SuggestedWord::from_row_fetch_original(row, db))
            })
            .optional()
            .unwrap();

        Span::current().record("found", &word.is_some());

        word
    }

    /// Returns the suggested word with examples and linked words populated.
    #[instrument(name = "Fetch full suggested word", fields(found), skip(db))]
    pub fn fetch_full(db: &impl UserAccessDb, id: u64) -> Option<SuggestedWord> {
        let mut word = SuggestedWord::fetch_alone(db, id);
        if let Some(w) = word.as_mut() {
            w.examples = SuggestedExample::fetch_all_for_suggestion(db, id);
            w.linked_words = SuggestedLinkedWord::fetch_all_for_suggestion(db, id);
        }

        Span::current().record("found", &word.is_some());

        word
    }

    /// Does not delete suggestion.
    #[instrument(
        name = "Accept just suggested word",
        fields(
            suggestion_id = self.suggestion_id,
            existing_id = self.word_id,
            accepted_id,
            hit = %self.to_plaintext(),
        ),
        skip_all
    )]
    pub fn accept_just_word_suggestion(&self, db: &impl ModeratorAccessDb) -> u64 {
        const INSERT: &str = "
            INSERT INTO words (
                word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
                is_inchoative, is_informal, transitivity, followed_by, noun_class, note
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                ON CONFLICT(word_id) DO UPDATE SET
                    english = excluded.english,
                    xhosa = excluded.xhosa,
                    part_of_speech = excluded.part_of_speech,
                    xhosa_tone_markings = excluded.xhosa_tone_markings,
                    infinitive = excluded.infinitive,
                    is_plural = excluded.is_plural,
                    noun_class = excluded.noun_class,
                    is_inchoative = excluded.is_inchoative,
                    is_informal = excluded.is_informal,
                    transitivity = excluded.transitivity,
                    followed_by = excluded.followed_by,
                    note = excluded.note
                RETURNING word_id;
        ";

        let conn = db.get().unwrap();
        let params = params![
            self.word_id,
            self.english.current(),
            self.xhosa.current(),
            self.part_of_speech.current(),
            self.xhosa_tone_markings.current(),
            self.infinitive.current(),
            self.is_plural.current(),
            self.is_inchoative.current(),
            self.is_informal.current(),
            self.transitivity.current(),
            self.followed_by.current().clone().unwrap_or_default(),
            self.noun_class.current().map(|x| x as u8),
            self.note.current(),
        ];

        let id: i64 = conn
            .prepare(INSERT)
            .unwrap()
            .query_row(params, |row| row.get("word_id"))
            .unwrap();
        let id = id as u64;

        add_attribution(db, &self.suggesting_user, WordId(id));

        Span::current().record("accepted_id", &id);

        id
    }

    #[instrument(name = "Accept whole word suggestion", skip_all)]
    pub fn accept_whole_word_suggestion(
        self,
        db: &impl ModeratorAccessDb,
        tantivy: Arc<TantivyClient>,
    ) {
        let word_suggestion_id = self.suggestion_id;
        let new_word_id = self.accept_just_word_suggestion(db);

        for mut example in self.examples.into_iter() {
            example.word_or_suggested_id = WordOrSuggestionId::existing(new_word_id);
            example.accept(db);
        }

        let old = WordOrSuggestionId::suggested(self.suggestion_id);
        let new = WordOrSuggestionId::existing(new_word_id);

        for mut l in self.linked_words.into_iter() {
            if l.first.current().0 == old {
                l.first = MaybeEdited::New((new, WordHit::empty()));
            } else {
                l.second = MaybeEdited::New((new, WordHit::empty()));
            }

            if l.first.current().0.is_existing() && l.second.current().0.is_existing() {
                l.accept(db);
            } else {
                l.update_first_and_second(db);
            }
        }

        let document = WordDocument {
            id: WordOrSuggestionId::existing(new_word_id),
            english: self.english.current().clone(),
            xhosa: self.xhosa.current().clone(),
            part_of_speech: *self.part_of_speech.current(),
            is_plural: *self.is_plural.current(),
            is_inchoative: *self.is_inchoative.current(),
            transitivity: *self.transitivity.current(),
            suggesting_user: None,
            noun_class: *self.noun_class.current(),
            is_informal: *self.is_informal.current()
        };

        let tantivy_clone = tantivy.clone();
        SuggestedWord::delete(db, tantivy_clone, word_suggestion_id);

        if self.word_id.is_none() {
            block_on(async move { tantivy.add_new_word(document).await });
        } else {
            block_on(async move { tantivy.edit_word(document).await });
        }
    }

    #[instrument(name = "Delete word suggestion", fields(found), skip(db, tantivy))]
    pub fn delete(db: &impl ModeratorAccessDb, tantivy: Arc<TantivyClient>, id: u64) -> bool {
        const DELETE: &str = "DELETE FROM word_suggestions WHERE suggestion_id = ?1;";

        block_on(async move { tantivy.delete_word(WordOrSuggestionId::suggested(id)).await });

        let conn = db.get().unwrap();
        let modified_rows = conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
        let found = modified_rows == 1;

        Span::current().record("found", &found);

        found
    }

    fn from_row_fetch_original(row: &Row<'_>, db: &impl UserAccessDb) -> Self {
        let existing_id = row.get::<&str, Option<i64>>("existing_word_id").unwrap();
        let e = existing_id.and_then(|id| ExistingWord::fetch_alone(db, id as u64));
        let e = e.as_ref();

        let val = row.get::<&str, Option<String>>("followed_by").unwrap();
        let old = e.and_then(|e| e.followed_by.clone());
        let followed_by = val.map(|x| x.parse().ok());
        let followed_by = match followed_by {
            Some(new) if e.is_none() => MaybeEdited::New(new),
            Some(new) if old != new => MaybeEdited::Edited { old, new },
            _ => MaybeEdited::Old(old),
        };

        SuggestedWord {
            suggesting_user: PublicUserInfo::try_from(row).unwrap(),
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
            is_inchoative: MaybeEdited::from_row("is_inchoative", row, e.map(|e| e.is_inchoative)),
            transitivity: MaybeEdited::from_row_with_sentinel(
                "transitivity",
                row,
                e.and_then(|e| e.transitivity),
            ),
            followed_by,
            noun_class: MaybeEdited::from_row_with_sentinel(
                "noun_class",
                row,
                e.and_then(|e| e.noun_class),
            ),
            note: MaybeEdited::from_row("note", row, e.map(|e| e.note.clone())),
            is_informal: MaybeEdited::from_row("is_informal", row, e.map(|e| e.is_plural)),
            examples: vec![],
            linked_words: vec![],
        }
    }

    #[instrument(
        level = "trace",
        name = "Fetch existing id for suggested word",
        fields(word_id),
        skip(db)
    )]
    pub fn fetch_existing_id_for_suggestion(
        db: &impl UserAccessDb,
        suggestion: u64,
    ) -> Option<u64> {
        const SELECT: &str =
            "SELECT existing_word_id FROM word_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        let word_id: Option<u64> = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![suggestion], |row| row.get("existing_word_id"))
            .unwrap();

        Span::current().record("word_id", &word_id.to_debug().as_str());

        word_id
    }
}

#[derive(Clone, Debug)]
pub struct SuggestedExample {
    pub changes_summary: String,
    pub suggesting_user: PublicUserInfo,

    pub suggestion_id: u64,
    pub existing_example_id: Option<u64>,
    pub word_or_suggested_id: WordOrSuggestionId,

    pub english: MaybeEdited<String>,
    pub xhosa: MaybeEdited<String>,
}

impl SuggestedExample {
    #[instrument(
        name = "Fetch all suggested examples for existing words",
        fields(results),
        skip(db)
    )]
    pub fn fetch_all_for_existing_words(
        db: &impl ModeratorAccessDb,
    ) -> impl Iterator<Item = (WordId, Vec<SuggestedExample>)> {
        const SELECT: &str = "
            SELECT words.word_id,
                   example_suggestions.suggestion_id, example_suggestions.existing_word_id,
                   example_suggestions.existing_example_id, example_suggestions.changes_summary,
                   example_suggestions.xhosa, example_suggestions.suggested_word_id,
                   example_suggestions.english, users.username, users.display_name,
                   example_suggestions.suggesting_user
            FROM example_suggestions
            INNER JOIN users ON example_suggestions.suggesting_user = users.user_id
            INNER JOIN words ON example_suggestions.existing_word_id = words.word_id;
        ";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let examples = query.query(params![]).unwrap();

        let mut map: HashMap<WordId, Vec<SuggestedExample>> = HashMap::new();

        examples
            .map(|row| {
                Ok((
                    WordId(row.get::<&str, u64>("word_id")?),
                    SuggestedExample::from_row_fetch_original(row, db),
                ))
            })
            .for_each(|(word_id, example)| {
                map.entry(word_id)
                    .or_insert_with(|| Vec::with_capacity(1))
                    .push(example);
                Ok(())
            })
            .unwrap();

        Span::current().record("results", &map.len());

        map.into_iter()
    }

    #[instrument(
        level = "trace",
        name = "Fetch all suggested examples for suggested word",
        fields(results),
        skip(db)
    )]
    pub fn fetch_all_for_suggestion(
        db: &impl UserAccessDb,
        suggested_word_id: u64,
    ) -> Vec<SuggestedExample> {
        const SELECT_SUGGESTION: &str = "
            SELECT
                suggestion_id, existing_word_id, suggested_word_id, existing_example_id,
                changes_summary, xhosa, english, username, display_name, suggesting_user
            FROM example_suggestions
            INNER JOIN users ON example_suggestions.suggesting_user = users.user_id
            WHERE suggested_word_id = ?1;
        ";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT_SUGGESTION).unwrap();
        let examples = query.query(params![suggested_word_id]).unwrap();

        let examples: Vec<_> = examples
            .map(|row| Ok(SuggestedExample::from_row_fetch_original(row, db)))
            .collect()
            .unwrap();

        Span::current().record("results", &examples.len());

        examples
    }

    #[instrument(name = "Fetch suggested example", fields(found), skip(db))]
    pub fn fetch(db: &impl UserAccessDb, suggestion_id: u64) -> Option<SuggestedExample> {
        const SELECT: &str = "
            SELECT
                suggestion_id, existing_word_id, suggested_word_id, existing_example_id,
                changes_summary, xhosa, english, username, display_name, suggesting_user
            FROM example_suggestions
            INNER JOIN users ON example_suggestions.suggesting_user = users.user_id
            WHERE suggestion_id = ?1;
        ";

        let conn = db.get().unwrap();
        let ex = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![suggestion_id], |row| {
                Ok(Self::from_row_fetch_original(row, db))
            })
            .optional()
            .unwrap();

        Span::current().record("found", &ex.is_some());

        ex
    }

    #[instrument(
        name = "Accept suggested example",
        fields(
            suggestion_id = self.suggestion_id,
            word_id = ?self.word_or_suggested_id,
            accepted_id,
        ),
        skip_all,
    )]
    pub fn accept(&self, db: &impl ModeratorAccessDb) -> i64 {
        const INSERT: &str = "
            INSERT INTO examples (example_id, word_id, english, xhosa) VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(example_id) DO UPDATE SET
                    english = excluded.english,
                    xhosa = excluded.xhosa
                RETURNING example_id;
        ";

        let conn = db.get().unwrap();
        let word = match self.word_or_suggested_id {
            WordOrSuggestionId::ExistingWord { existing_id } => existing_id,
            _ => panic!("No existing word for suggested example {:#?}", self),
        };
        let params = params![
            self.existing_example_id,
            word,
            self.english.current(),
            self.xhosa.current()
        ];

        let id = conn
            .prepare(INSERT)
            .unwrap()
            .query_row(params, |row| row.get("example_id"))
            .unwrap();

        add_attribution(db, &self.suggesting_user, WordId(word));
        SuggestedExample::delete(db, self.suggestion_id);

        Span::current().record("accepted_id", &id);

        id
    }

    #[instrument(name = "Delete suggested example", fields(found), skip(db))]
    pub fn delete(db: &impl ModeratorAccessDb, id: u64) -> bool {
        const DELETE: &str = "DELETE FROM example_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        let modified_rows = conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
        let found = modified_rows == 1;
        Span::current().record("found", &found);
        found
    }

    fn from_row_fetch_original(row: &Row<'_>, db: &impl UserAccessDb) -> Self {
        let existing_id = row.get::<&str, Option<i64>>("existing_example_id").unwrap();
        let e = existing_id.and_then(|id| ExistingExample::fetch(db, id as u64));
        let e = e.as_ref();

        SuggestedExample {
            suggesting_user: PublicUserInfo::try_from(row).unwrap(),
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
    pub suggesting_user: PublicUserInfo,
    pub suggestion_id: u64,
    pub existing_linked_word_id: Option<u64>,

    pub first: MaybeEdited<(WordOrSuggestionId, WordHit)>,
    pub second: MaybeEdited<(WordOrSuggestionId, WordHit)>,
    pub link_type: MaybeEdited<WordLinkType>,
}

impl SuggestedLinkedWord {
    #[instrument(name = "Fetch suggested linked word", skip(db))]
    pub fn fetch(db: &impl UserAccessDb, suggestion: u64) -> SuggestedLinkedWord {
        const SELECT_SUGGESTION: &str = "
            SELECT linked_word_suggestions.suggestion_id, linked_word_suggestions.link_type,
                   linked_word_suggestions.changes_summary, linked_word_suggestions.existing_linked_word_id,
                   linked_word_suggestions.first_existing_word_id, linked_word_suggestions.second_existing_word_id,
                   linked_word_suggestions.suggested_word_id,
                   linked_word_suggestions.second_suggested_word_id,
                   linked_words.first_word_id, linked_words.second_word_id, users.username,
                   users.display_name, linked_word_suggestions.suggesting_user
            FROM linked_word_suggestions
            LEFT JOIN linked_words ON existing_linked_word_id = link_id
            INNER JOIN users ON linked_word_suggestions.suggesting_user = users.user_id
            WHERE suggestion_id = ?1;
        ";

        let conn = db.get().unwrap();
        let s = conn
            .prepare(SELECT_SUGGESTION)
            .unwrap()
            .query_row(params![suggestion], |row| {
                Ok(SuggestedLinkedWord::from_row_populate_both(row, db))
            })
            .unwrap();
        s
    }

    /// Each link will show up for either suggestion.
    #[instrument(
        level = "trace",
        name = "Fetch all suggested linked words for suggested word",
        fields(results),
        skip(db)
    )]
    pub fn fetch_all_for_suggestion(
        db: &impl UserAccessDb,
        suggested_word_id: u64,
    ) -> Vec<SuggestedLinkedWord> {
        const SELECT_SUGGESTION: &str = "
            SELECT suggestion_id, link_type, changes_summary, existing_linked_word_id,
                first_existing_word_id, second_existing_word_id, suggested_word_id,
                second_suggested_word_id, username, display_name, suggesting_user
            FROM linked_word_suggestions
            INNER JOIN users ON linked_word_suggestions.suggesting_user = users.user_id
            WHERE suggested_word_id = ?1 OR second_suggested_word_id = ?1;
        ";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT_SUGGESTION).unwrap();
        let rows = query.query(params![suggested_word_id]).unwrap();

        let mut vec: Vec<SuggestedLinkedWord> = rows
            .map(|row| Ok(SuggestedLinkedWord::from_row_populate_both(row, db)))
            .collect()
            .unwrap();

        Span::current().record("results", &vec.len());
        vec.sort_by_key(|link| *link.link_type.current());
        vec
    }

    /// Each link shows up once and only once.
    #[instrument(
        name = "Fetch all suggested linked words for existing words",
        fields(results),
        skip_all
    )]
    pub fn fetch_all_for_existing_words(
        db: &impl ModeratorAccessDb,
    ) -> impl Iterator<Item = (WordId, Vec<SuggestedLinkedWord>)> {
        const SELECT: &str = "
            SELECT words.word_id,
                   linked_word_suggestions.suggestion_id, linked_word_suggestions.link_type,
                   linked_word_suggestions.changes_summary, linked_word_suggestions.existing_linked_word_id,
                   linked_word_suggestions.first_existing_word_id, linked_word_suggestions.second_existing_word_id,
                   linked_word_suggestions.suggested_word_id, linked_word_suggestions.suggesting_user,
                   linked_word_suggestions.second_suggested_word_id,
                   linked_words.first_word_id, linked_words.second_word_id, users.username,
                   users.display_name
            FROM linked_word_suggestions

            INNER JOIN users ON linked_word_suggestions.suggesting_user = users.user_id

            LEFT JOIN
                linked_words
                ON linked_word_suggestions.existing_linked_word_id = linked_words.link_id

            INNER JOIN
                words
                ON linked_word_suggestions.first_existing_word_id = words.word_id OR
                   linked_words.first_word_id = words.word_id

            WHERE
                linked_word_suggestions.suggested_word_id IS NULL AND
                linked_word_suggestions.second_suggested_word_id IS NULL;
        ";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let examples = query.query(params![]).unwrap();

        let mut map: HashMap<WordId, Vec<SuggestedLinkedWord>> = HashMap::new();

        examples
            .map(|row| {
                let (first, second, fallback_first) = (
                    row.get::<&str, Option<u64>>("first_existing_word_id")?,
                    row.get::<&str, Option<u64>>("second_existing_word_id")?,
                    row.get::<&str, Option<u64>>("first_word_id")?,
                );

                let chosen = first.or(second).or(fallback_first);

                Ok((
                    WordId(chosen.unwrap()),
                    SuggestedLinkedWord::from_row_populate_both(row, db),
                ))
            })
            .for_each(|(word_id, link)| {
                map.entry(word_id)
                    .or_insert_with(|| Vec::with_capacity(1))
                    .push(link);
                Ok(())
            })
            .unwrap();

        Span::current().record("results", &map.len());

        map.into_iter()
    }

    // TODO(error handling)
    #[instrument(
        name = "Accept suggested linked word",
        fields(
            suggestion_id = self.suggestion_id,
            existing_id = self.existing_linked_word_id,
            accepted_id,
        )
        skip_all,
    )]
    pub fn accept(&self, db: &impl ModeratorAccessDb) -> i64 {
        const INSERT: &str = "
            INSERT INTO linked_words (link_id, link_type, first_word_id, second_word_id)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(link_id) DO UPDATE SET
                    link_type = excluded.link_type
                RETURNING link_id;
        ";

        let conn = db.get().unwrap();

        let get_existing = |a: &MaybeEdited<(_, _)>| match a.current().0 {
            WordOrSuggestionId::ExistingWord { existing_id } => existing_id,
            _ => panic!("No existing word for suggested linked word {:#?}", self),
        };

        let (first, second) = (get_existing(&self.first), get_existing(&self.second));

        let params = params![
            self.existing_linked_word_id,
            self.link_type.current(),
            first,
            second,
        ];

        let id = conn
            .prepare(INSERT)
            .unwrap()
            .query_row(params, |row| row.get("link_id"))
            .unwrap();

        add_attribution(db, &self.suggesting_user, WordId(first));
        add_attribution(db, &self.suggesting_user, WordId(second));
        SuggestedLinkedWord::delete(db, self.suggestion_id);

        Span::current().record("accepted_id", &id);

        id
    }

    #[instrument(
        level = "trace",
        name = "Update suggested linked word first and second",
        fields(suggestion_id = self.suggestion_id),
        skip_all
    )]
    pub fn update_first_and_second(&self, db: &impl ModeratorAccessDb) {
        const UPDATE: &str = "
            UPDATE linked_word_suggestions
            SET first_existing_word_id = ?1, second_existing_word_id = ?2,
                suggested_word_id = ?3, second_suggested_word_id = ?4
            WHERE suggestion_id = ?5;
        ";

        let (first, second) = (self.first.current().0, self.second.current().0);
        let params = params![
            first.into_existing(),
            second.into_existing(),
            first.into_suggested(),
            second.into_suggested(),
            self.suggestion_id,
        ];

        let conn = db.get().unwrap();
        conn.prepare(UPDATE).unwrap().execute(params).unwrap();
    }

    #[instrument(name = "Delete suggested linked word", fields(found), skip(db))]
    pub fn delete(db: &impl ModeratorAccessDb, id: u64) -> bool {
        const DELETE: &str = "DELETE FROM linked_word_suggestions WHERE suggestion_id = ?1;";

        let conn = db.get().unwrap();
        let modified_rows = conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
        let found = modified_rows == 1;
        Span::current().record("found", &found);
        found
    }

    #[instrument(
        level = "trace",
        name = "Populate suggested linked word",
        fields(suggestion_id),
        skip_all
    )]
    fn from_row_populate_both(row: &Row<'_>, db: &impl UserAccessDb) -> Self {
        const SELECT: &str =
            "SELECT link_type, first_word_id, second_word_id FROM linked_words WHERE link_id = ?1;";

        let conn = db.get().unwrap();
        let existing_id = row
            .get::<&str, Option<i64>>("existing_linked_word_id")
            .unwrap();
        let (other_type, other_first, other_second) = if let Some(id) = existing_id {
            let trio = conn
                .prepare(SELECT)
                .unwrap()
                .query_row(params![id], |r| {
                    Ok((
                        r.get("link_type")?,
                        r.get("first_word_id")?,
                        r.get("second_word_id")?,
                    ))
                })
                .unwrap();
            (Some(trio.0), Some(trio.1), Some(trio.2))
        } else {
            (None, None, None)
        };

        let ignore_invalid_col = |e| match e {
            rusqlite::Error::InvalidColumnName(n)
                if n == "first_word_id" || n == "second_word_id" => {}
            _ => panic!("Error: {:#?}", e),
        };

        let existing = |col| {
            row.get::<&str, Option<u64>>(col)
                .map_err(ignore_invalid_col)
                .ok()
                .flatten()
                .map(WordOrSuggestionId::existing)
        };
        let suggested = |col| {
            row.get::<&str, Option<u64>>(col)
                .map_err(ignore_invalid_col)
                .ok()
                .flatten()
                .map(WordOrSuggestionId::suggested)
        };

        let word_ids: [Option<WordOrSuggestionId>; 6] = [
            existing("first_existing_word_id"),
            existing("second_existing_word_id"),
            suggested("suggested_word_id"),
            suggested("second_suggested_word_id"),
            existing("first_word_id"),
            existing("second_word_id"),
        ];

        let mut iter = word_ids.into_iter().flatten().take(2);

        let mut next = |other_id| {
            let this_id = iter.next().unwrap();
            let get_other =
                |id| WordHit::fetch_from_db(db, WordOrSuggestionId::existing(id)).unwrap();

            match other_id {
                Some(other_id) if WordOrSuggestionId::existing(other_id) == this_id => {
                    MaybeEdited::Old((WordOrSuggestionId::existing(other_id), get_other(other_id)))
                }
                Some(other_id) => MaybeEdited::Edited {
                    old: (WordOrSuggestionId::existing(other_id), get_other(other_id)),
                    new: (this_id, WordHit::fetch_from_db(db, this_id).unwrap()),
                },
                None => MaybeEdited::New((this_id, WordHit::fetch_from_db(db, this_id).unwrap())),
            }
        };

        let (first, second) = (next(other_first), next(other_second));

        let suggestion_id = row.get("suggestion_id").unwrap();

        Span::current().record("suggestion_id", &suggestion_id);

        SuggestedLinkedWord {
            suggesting_user: PublicUserInfo::try_from(row).unwrap(),
            changes_summary: row.get("changes_summary").unwrap(),
            suggestion_id,
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

#[derive(Copy, Clone, Debug)]
pub enum MaybeEdited<T> {
    Edited { old: T, new: T },
    Old(T),
    New(T),
}

impl<T> MaybeEdited<T> {
    pub fn map<'a, U, F: Fn(&'a T) -> U>(&'a self, f: F) -> MaybeEdited<U> {
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
}

impl<T: PartialEq> MaybeEdited<T> {
    pub fn was_or_is(&self, other: &T) -> bool {
        match self {
            MaybeEdited::Edited { new, old } => new == other || old == other,
            MaybeEdited::New(new) => new == other,
            MaybeEdited::Old(old) => old == other,
        }
    }
}

impl<T> MaybeEdited<Option<T>> {
    pub fn is_none(&self) -> bool {
        use MaybeEdited::*;
        matches!(
            self,
            Edited {
                old: None,
                new: None
            } | Old(None)
                | New(None)
        )
    }
}

impl<T: Default + Clone> MaybeEdited<Option<T>> {
    pub fn map_or_default(&self) -> MaybeEdited<T> {
        self.map(|opt| opt.clone().unwrap_or_default())
    }
}

impl<T: Debug> MaybeEdited<Option<T>> {
    pub fn map_debug(&self) -> MaybeEdited<String> {
        self.map(|opt| match opt {
            Some(v) => format!("{:?}", v),
            None => String::new(),
        })
    }
}

impl MaybeEdited<WordHit> {
    pub fn hyperlinked(&self) -> MaybeEdited<HyperlinkWrapper<'_>> {
        self.map(HyperlinkWrapper)
    }
}


impl<T: FromSql + PartialEq + Eq> MaybeEdited<T> {
    fn from_row(idx: &str, row: &Row<'_>, existing: Option<T>) -> MaybeEdited<T> {
        match (row.get::<&str, Option<T>>(idx).unwrap(), existing) {
            (Some(new), Some(old)) if new != old => MaybeEdited::Edited { old, new },
            (None | Some(_), Some(old)) => MaybeEdited::Old(old),
            (Some(new), None) => MaybeEdited::New(new),
            (None, None) => panic!(
                "Field in suggestion unfilled; this is an error! Suggestion id: {:?}. Index: {}",
                row.get::<&str, i64>("suggestion_id"),
                idx,
            ),
        }
    }
}

impl<T> MaybeEdited<T>
where
    T: TryFromPrimitive,
    T::Primitive: TryFrom<i64>,
{
    fn from_row_with_sentinel(idx: &str, row: &Row<'_>, old: Option<T>) -> MaybeEdited<Option<T>> {
        let res = row.get::<&str, Option<WithDeleteSentinel<T>>>(idx);
        match res {
            Ok(Some(WithDeleteSentinel::Remove)) if old.is_some() => {
                MaybeEdited::Edited { old, new: None }
            }
            Ok(Some(WithDeleteSentinel::Remove)) => MaybeEdited::Old(None),
            Ok(Some(WithDeleteSentinel::Some(new))) if old.is_none() => MaybeEdited::New(Some(new)),
            Ok(Some(WithDeleteSentinel::Some(new))) => MaybeEdited::Edited {
                old,
                new: Some(new),
            },
            Ok(None) => MaybeEdited::Old(old),
            Err(e) => panic!(
                "Invalid {} discriminator in database: {:?}",
                std::any::type_name::<T>(),
                e
            ),
        }
    }
}
impl<T: DisplayHtml> DisplayHtml for MaybeEdited<T> {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        match self {
            MaybeEdited::Edited { new, old } => {
                f.fmt.write_str("<ins>")?;
                if new.is_empty_str() {
                    f.fmt.write_str("[Removed]")?;
                } else {
                    new.fmt(f)?;
                }
                f.fmt.write_str("</ins> ")?;

                f.fmt.write_str("<del>")?;
                if old.is_empty_str() {
                    f.fmt.write_str("[None]")?;
                } else {
                    old.fmt(f)?;
                }
                f.fmt.write_str("</del>")
            }
            MaybeEdited::Old(old) => old.fmt(f),
            MaybeEdited::New(new) => {
                f.fmt.write_str("<ins>")?;
                new.fmt(f)?;
                f.fmt.write_str("</ins>")
            }
        }
    }

    fn is_empty_str(&self) -> bool {
        match self {
            MaybeEdited::Edited { new, old } => new.is_empty_str() && old.is_empty_str(),
            MaybeEdited::Old(v) => v.is_empty_str(),
            MaybeEdited::New(v) => v.is_empty_str(),
        }
    }
}

trait TextIfBoolIn {
    fn into_maybe_edited(self) -> MaybeEdited<bool>;
}

impl TextIfBoolIn for bool {
    fn into_maybe_edited(self) -> MaybeEdited<bool> {
        MaybeEdited::Old(self)
    }
}

impl TextIfBoolIn for MaybeEdited<bool> {
    fn into_maybe_edited(self) -> MaybeEdited<bool> {
        self
    }
}

fn text_if_bool<T: TextIfBoolIn>(
    yes: &'static str,
    no: &'static str,
    b: T,
    show_no_when_new: bool,
) -> MaybeEdited<&'static str> {
    match b.into_maybe_edited() {
        MaybeEdited::Edited { new, old } => MaybeEdited::Edited {
            new: if new { yes } else { no },
            old: if old { yes } else { no },
        },
        MaybeEdited::New(b) if show_no_when_new => MaybeEdited::New(if b { yes } else { no }),
        MaybeEdited::New(b) if b => MaybeEdited::New(yes),
        MaybeEdited::Old(b) if b => MaybeEdited::Old(yes),
        _ => MaybeEdited::Old(""),
    }
}

impl DisplayHtml for SuggestedWord {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        DisplayHtml::fmt(&self.english, f)?;
        f.fmt.write_str(" - <span lang=\"xh\">")?;
        DisplayHtml::fmt(&self.xhosa, f)?;
        f.fmt.write_str("</span> (")?;

        f.join_if_non_empty(
            " ",
            [
                &text_if_bool("informal", "non-informal", self.is_informal, false),
                &text_if_bool(
                    "inchoative",
                    "non-inchoative",
                    self.is_inchoative,
                    self.part_of_speech.was_or_is(&PartOfSpeech::Verb),
                ),
                &self
                    .transitivity
                    .map(|x| x.map(|x| Transitivity::explicit_moderation_page(&x)))
                    as &dyn DisplayHtml,
                &text_if_bool(
                    "plural",
                    "singular",
                    self.is_plural,
                    self.part_of_speech.was_or_is(&PartOfSpeech::Noun),
                ),
                &self.part_of_speech,
                &self.noun_class.map(|opt| opt.map(NounClassInHit)),
            ],
        )?;
        f.write_text(")")
    }

    fn is_empty_str(&self) -> bool {
        false
    }
}