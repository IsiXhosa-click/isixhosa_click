use crate::auth::FullUser;
use crate::database::suggestion::{SuggestedExample, SuggestedLinkedWord, SuggestedWord};
use crate::database::WordId;
use crate::database::WordOrSuggestionId;
use crate::search::{TantivyClient, WordDocument};
use crate::serialization::{deserialize_checkbox, false_fn};
use crate::spawn_blocking_child;
use futures::executor::block_on;
use isixhosa::noun::NounClass;
use isixhosa_common::database::UserAccessDb;
use isixhosa_common::language::{ConjunctionFollowedBy, PartOfSpeech, Transitivity, WordLinkType};
use isixhosa_common::types::{ExistingExample, ExistingLinkedWord, ExistingWord, WordHit};
use rusqlite::types::{ToSqlOutput, Value};
use rusqlite::{params, ToSql};
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::{serde_as, NoneAsEmptyString};
use std::num::NonZeroU64;
use std::sync::Arc;
use tracing::{debug_span, instrument, Span};

fn diff<T: PartialEq + Eq>(value: T, template: &T, override_use_value: bool) -> Option<T> {
    if override_use_value || &value != template {
        Some(value)
    } else {
        None
    }
}

fn diff_opt<T: PartialEq + Eq>(
    value: T,
    template: &Option<T>,
    override_use_value: bool,
) -> Option<T> {
    if override_use_value || Some(&value) != template.as_ref() || template.is_none() {
        Some(value)
    } else {
        None
    }
}

fn diff_with_sentinel<T>(value: Option<T>, template: Option<T>) -> ToSqlOutput<'static>
where
    T: PartialEq + Eq + Copy + Into<u8>,
{
    match value {
        Some(v) if value != template => ToSqlOutput::Owned(Value::Integer(v.into() as i64)),
        None if template.is_some() => 255u8.to_sql().unwrap(),
        _ => None::<u8>.to_sql().unwrap(),
    }
}

#[instrument(level = "trace", name = "Suggest word deletion", skip(db))]
pub async fn suggest_word_deletion(
    suggesting_user: &FullUser,
    word_id: WordId,
    db: &impl UserAccessDb,
) {
    const STATEMENT: &str =
        "INSERT INTO word_deletion_suggestions (word_id, reason, suggesting_user) VALUES (?1, ?2, ?3);";

    let db = db.clone();
    let user_id = suggesting_user.id.get();

    spawn_blocking_child(move || {
        let conn = db.get().unwrap();
        conn.prepare(STATEMENT)
            .unwrap()
            .execute(params![word_id.0, "No reason given", user_id])
            .unwrap();
    })
    .await
    .unwrap()
}

#[instrument(
    name = "Process word submission",
    fields(suggestion_id, changes),
    skip_all
)]
pub async fn submit_suggestion(
    word: WordSubmission,
    tantivy: Arc<TantivyClient>,
    suggesting_user: &FullUser,
    db: &impl UserAccessDb,
) {
    // Intentionally suggesting_user is not set to excluded
    const INSERT_SUGGESTION: &str = "
        INSERT INTO word_suggestions (
            suggestion_id, suggesting_user, existing_word_id, changes_summary, english, xhosa,
            part_of_speech, xhosa_tone_markings, infinitive, is_plural, is_inchoative, is_informal,
            transitivity, followed_by, noun_class, note
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(suggestion_id) DO UPDATE SET
                existing_word_id = excluded.existing_word_id,
                changes_summary = excluded.changes_summary,
                english = excluded.english,
                xhosa = excluded.xhosa,
                part_of_speech = excluded.part_of_speech,
                xhosa_tone_markings = excluded.xhosa_tone_markings,
                infinitive = excluded.infinitive,
                is_plural = excluded.is_plural,
                is_inchoative = excluded.is_inchoative,
                is_informal = excluded.is_informal,
                transitivity = excluded.transitivity,
                followed_by = excluded.followed_by,
                noun_class = excluded.noun_class,
                note = excluded.note
            RETURNING suggestion_id;
        ";

    let db = db.clone();
    let mut w = word;
    let suggesting_user = suggesting_user.id;

    if w.infinitive.starts_with('U') {
        w.infinitive = w.infinitive.replacen('U', "u", 1);
    }

    spawn_blocking_child(move || {
        let conn = db.get().unwrap();

        let orig = WordFormTemplate::fetch_from_db(&db, w.existing_id, None).unwrap_or_default();
        let use_submitted = w.existing_id.is_none();

        let changes_summary_default = if w.existing_id.is_none() {
            "Word added."
        } else {
            "Word edited."
        };
        let changes_summary = w
            .changes_summary
            .clone()
            .unwrap_or_else(|| changes_summary_default.to_owned());

        let params = params![
            w.suggestion_id,
            suggesting_user.get(),
            w.existing_id,
            changes_summary,
            diff(w.english.clone(), &orig.english, use_submitted),
            diff(w.xhosa.clone(), &orig.xhosa, use_submitted),
            diff_opt(w.part_of_speech, &orig.part_of_speech, use_submitted),
            diff(
                w.xhosa_tone_markings.clone(),
                &orig.xhosa_tone_markings,
                use_submitted
            ),
            diff(w.infinitive.clone(), &orig.infinitive, use_submitted),
            diff(w.is_plural, &orig.is_plural, use_submitted),
            diff(w.is_inchoative, &orig.is_inchoative, use_submitted),
            diff(w.is_informal, &orig.is_informal, use_submitted),
            diff_with_sentinel(w.transitivity, orig.transitivity),
            diff(w.followed_by.clone(), &orig.followed_by, use_submitted),
            diff_with_sentinel(w.noun_class, orig.noun_class),
            diff(w.note.clone(), &orig.note, use_submitted)
        ];

        let orig_suggestion = WordFormTemplate::fetch_from_db(&db, None, w.suggestion_id);

        let any_changes = match orig_suggestion.as_ref() {
            Some(orig_suggestion) => w.has_any_changes_in_word(orig_suggestion),
            None => w.has_any_changes_in_word(&orig),
        };

        let suggested_word_id = if any_changes {
            let _g = debug_span!("Insert word suggestion").entered();
            let suggested_word_id: i64 = conn
                .prepare(INSERT_SUGGESTION)
                .unwrap()
                .query_row(params, |row| row.get("suggestion_id"))
                .unwrap();
            Some(suggested_word_id)
        } else {
            w.suggestion_id.map(|id| id as i64)
        };

        let span = Span::current();
        span.record("changes", any_changes);
        span.record("suggestion_id", suggested_word_id);

        let suggested_word_id_if_new = suggested_word_id.filter(|_| w.existing_id.is_none());

        // Don't need to index non-new word suggestions
        if let Some(suggested_word_id) = suggested_word_id_if_new {
            let doc = WordDocument {
                id: WordOrSuggestionId::suggested(suggested_word_id as u64),
                english: w.english.clone(),
                xhosa: w.xhosa.clone(),
                part_of_speech: w.part_of_speech,
                is_plural: w.is_plural,
                is_inchoative: w.is_inchoative,
                transitivity: w.transitivity,
                suggesting_user: Some(suggesting_user),
                noun_class: w.noun_class,
                is_informal: w.is_informal,
            };

            if orig_suggestion.is_none() {
                block_on(async move { tantivy.add_new_word(doc).await });
            } else if matches!(orig_suggestion, Some(o) if w.has_any_changes_in_word(&o)) {
                block_on(async move { tantivy.edit_word(doc).await });
            }
        }

        process_linked_words(
            &mut w,
            &db,
            suggesting_user,
            suggested_word_id_if_new,
            &changes_summary,
        );
        process_examples(
            &mut w,
            &db,
            suggesting_user,
            suggested_word_id_if_new,
            &changes_summary,
        );
    })
    .await
    .unwrap();
}

#[instrument(
    name = "Process linked words submissions",
    fields(
        suggested_word_id = suggested_word_id_if_new,
        existing_word_id = w.existing_id,
        added,
        edited,
        deleted,
        skipped,
    ),
    skip_all
)]
fn process_linked_words(
    w: &mut WordSubmission,
    db: &impl UserAccessDb,
    suggesting_user: NonZeroU64,
    suggested_word_id_if_new: Option<i64>,
    changes_summary: &str,
) {
    const INSERT_LINKED_WORD_SUGGESTION: &str = "
        INSERT INTO linked_word_suggestions (
            suggestion_id, suggesting_user, existing_linked_word_id, changes_summary,
            suggested_word_id, second_suggested_word_id, link_type, first_existing_word_id,
            second_existing_word_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(suggestion_id) DO UPDATE SET
                changes_summary = excluded.changes_summary,
                suggested_word_id = excluded.suggested_word_id,
                second_suggested_word_id = excluded.second_suggested_word_id,
                link_type = excluded.link_type,
                first_existing_word_id = excluded.first_existing_word_id,
                second_existing_word_id = excluded.second_existing_word_id;
        ";

    const DELETE_LINKED_WORD_SUGGESTION: &str =
        "DELETE FROM linked_word_suggestions WHERE suggestion_id = ?1;";

    const SUGGEST_LINKED_WORD_DELETION: &str = "
            INSERT INTO linked_word_deletion_suggestions (linked_word_id, reason, suggesting_user)
            VALUES (?1, ?2, ?3);
         ";

    let use_submitted = w.existing_id.is_none() && w.suggestion_id.is_none();

    let conn = db.get().unwrap();
    let mut upsert_suggested_link = conn.prepare(INSERT_LINKED_WORD_SUGGESTION).unwrap();
    let mut delete_suggested_link = conn.prepare(DELETE_LINKED_WORD_SUGGESTION).unwrap();
    let mut suggest_link_deletion = conn.prepare(SUGGEST_LINKED_WORD_DELETION).unwrap();

    let existing_word_id = w.existing_id;
    let suggestion_id = w.suggestion_id;

    let [mut deleted, mut edited, mut skipped] = [0u32; 3];

    let mut maybe_insert_link = |new: LinkedWordSubmission, old: Option<ExistingLinkedWord>| {
        if !new.has_any_changes(&old) {
            skipped += 1;
            return;
        } else {
            edited += 1;
        }

        let (first, second) = match &old {
            Some(old) => match existing_word_id {
                Some(this_id) if this_id == old.first_word_id => {
                    let second = diff(
                        new.other,
                        &WordOrSuggestionId::existing(old.second_word_id),
                        use_submitted,
                    );
                    (None, second)
                }
                Some(_) => {
                    let first = diff(
                        new.other,
                        &WordOrSuggestionId::existing(old.first_word_id),
                        use_submitted,
                    );
                    (first, None)
                }
                None => {
                    panic!("This `existing_id` is none, but there is an old linked word!");
                }
            },
            None => {
                let existing = existing_word_id.map(WordOrSuggestionId::existing);
                let suggested = suggestion_id.map(WordOrSuggestionId::suggested);
                (existing.or(suggested), Some(new.other))
            }
        };

        let first_existing = first.and_then(WordOrSuggestionId::into_existing);
        let first_suggested = first.and_then(WordOrSuggestionId::into_suggested);
        let second_existing = second.and_then(WordOrSuggestionId::into_existing);
        let second_suggested = second.and_then(WordOrSuggestionId::into_suggested);

        upsert_suggested_link
            .execute(params![
                new.suggestion_id,
                suggesting_user.get(),
                new.existing_id,
                changes_summary,
                first_suggested,
                second_suggested,
                diff_opt(
                    new.link_type,
                    &old.as_ref().map(|o| o.link_type),
                    use_submitted
                ),
                first_existing,
                second_existing,
            ])
            .unwrap();
    };

    let linked_words = &mut w.linked_words.0;

    match (w.suggestion_id, w.existing_id) {
        // Editing a new suggested word
        (Some(suggested), None) => {
            for prev in SuggestedLinkedWord::fetch_all_for_suggestion(db, suggested) {
                if let Some(i) = linked_words
                    .iter()
                    .position(|new| new.suggestion_id == Some(prev.suggestion_id))
                {
                    let new = linked_words.remove(i);
                    let old = new.existing_id.and_then(|id| {
                        ExistingLinkedWord::fetch(db, id, existing_word_id.unwrap())
                    });
                    maybe_insert_link(new, old);
                } else {
                    deleted += 1;
                    delete_suggested_link
                        .execute(params![prev.suggestion_id])
                        .unwrap();
                }
            }
        }
        // Editing an edit to an existing word, or editing an existing word
        (_, Some(existing)) => {
            for prev in ExistingLinkedWord::fetch_all_for_word(db, existing) {
                if let Some(i) = linked_words
                    .iter()
                    .position(|new| new.existing_id == Some(prev.link_id))
                {
                    let new = linked_words.remove(i);
                    maybe_insert_link(new, Some(prev));
                } else {
                    deleted += 1;
                    suggest_link_deletion
                        .execute(params![
                            prev.link_id,
                            w.changes_summary,
                            suggesting_user.get()
                        ])
                        .unwrap();
                }
            }
        }
        // Brand new word submission
        (None, None) => {}
    }

    // Newly added linked words
    for new in &w.linked_words.0 {
        let other_existing = new.other.into_existing();
        let other_suggested = new.other.into_suggested();

        upsert_suggested_link
            .execute(params![
                new.suggestion_id,
                suggesting_user.get(),
                new.existing_id,
                changes_summary,
                suggested_word_id_if_new,
                other_suggested,
                new.link_type,
                w.existing_id,
                other_existing,
            ])
            .unwrap();
    }

    let span = Span::current();
    span.record("added", w.linked_words.0.len());
    span.record("edited", edited);
    span.record("deleted", deleted);
    span.record("skipped", skipped);
}

#[instrument(
    name = "Process example submissions",
    fields(
        suggested_word_id = suggested_word_id_if_new,
        existing_word_id = w.existing_id,
        added,
        edited,
        deleted,
        skipped,
    ),
    skip_all
)]
fn process_examples(
    w: &mut WordSubmission,
    db: &impl UserAccessDb,
    suggesting_user: NonZeroU64,
    suggested_word_id_if_new: Option<i64>,
    changes_summary: &str,
) {
    const INSERT_EXAMPLE_SUGGESTION: &str = "
        INSERT INTO example_suggestions (
            suggestion_id, suggesting_user, existing_example_id, changes_summary, suggested_word_id,
            existing_word_id, english, xhosa
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(suggestion_id) DO UPDATE SET
                changes_summary = excluded.changes_summary,
                suggested_word_id = excluded.suggested_word_id,
                existing_word_id = excluded.existing_word_id,
                english = excluded.english,
                xhosa = excluded.xhosa;
        ";

    const DELETE_EXAMPLE_SUGGESTION: &str =
        "DELETE FROM example_suggestions WHERE suggestion_id = ?1;";

    const SUGGEST_EXAMPLE_DELETION: &str = "
            INSERT INTO example_deletion_suggestions (example_id, reason, suggesting_user)
            VALUES (?1, ?2, ?3);
        ";

    let conn = db.get().unwrap();
    let mut upsert_example = conn.prepare(INSERT_EXAMPLE_SUGGESTION).unwrap();
    let mut delete_suggested_example = conn.prepare(DELETE_EXAMPLE_SUGGESTION).unwrap();
    let mut suggest_example_deletion = conn.prepare(SUGGEST_EXAMPLE_DELETION).unwrap();

    let use_submitted = w.existing_id.is_none() && w.suggestion_id.is_none();
    let existing_id = w.existing_id;
    let examples = &mut w.examples;

    let [mut deleted, mut edited, mut skipped] = [0u32; 3];

    let mut maybe_insert_example = |new: ExampleSubmission, old: Option<ExistingExample>| {
        if !new.has_any_changes(&old) {
            skipped += 1;
            return;
        } else {
            edited += 1;
        }

        upsert_example
            .execute(params![
                new.suggestion_id,
                suggesting_user.get(),
                new.existing_id,
                changes_summary,
                suggested_word_id_if_new,
                existing_id,
                diff_opt(
                    new.english,
                    &old.as_ref().map(|o| o.english.clone()),
                    use_submitted
                ),
                diff_opt(
                    new.xhosa,
                    &old.as_ref().map(|o| o.xhosa.clone()),
                    use_submitted
                ),
            ])
            .unwrap();
    };

    match (w.suggestion_id, w.existing_id) {
        (Some(suggested), None) => {
            for prev in SuggestedExample::fetch_all_for_suggestion(db, suggested) {
                if let Some(i) = examples
                    .iter()
                    .position(|new| new.suggestion_id == Some(prev.suggestion_id))
                {
                    let new = examples.remove(i);
                    let old = new
                        .existing_id
                        .and_then(|id| ExistingExample::fetch(db, id));
                    maybe_insert_example(new, old);
                } else {
                    deleted += 1;
                    delete_suggested_example
                        .execute(params![prev.suggestion_id])
                        .unwrap();
                }
            }
        }
        (_, Some(existing)) => {
            for prev in ExistingExample::fetch_all_for_word(db, existing) {
                let new = examples
                    .iter()
                    .position(|new| new.existing_id == Some(prev.example_id))
                    .map(|idx| examples.remove(idx))
                    .filter(|new| !(new.english.is_empty() && new.xhosa.is_empty()));

                match new {
                    Some(new) => maybe_insert_example(new, Some(prev)),
                    None => {
                        deleted += 1;
                        suggest_example_deletion
                            .execute(params![
                                prev.example_id,
                                w.changes_summary,
                                suggesting_user.get()
                            ])
                            .unwrap();
                    }
                }
            }
        }
        (None, None) => {}
    }

    for new in &mut w.examples {
        new.english = new.english.trim().to_owned();
        new.xhosa = new.xhosa.trim().to_owned();

        if new.english.is_empty() && new.xhosa.is_empty() {
            continue;
        }

        const PUNCTUATION: [char; 4] = ['.', '?', '!', '"'];

        if !new.english.ends_with(&PUNCTUATION[..]) {
            new.english.push('.');
        }

        if !new.xhosa.ends_with(&PUNCTUATION[..]) {
            new.xhosa.push('.');
        }

        upsert_example
            .execute(params![
                new.suggestion_id,
                suggesting_user.get(),
                new.existing_id,
                changes_summary,
                suggested_word_id_if_new,
                w.existing_id,
                new.english,
                new.xhosa
            ])
            .unwrap();
    }

    let span = Span::current();
    span.record("added", w.examples.len());
    span.record("edited", edited);
    span.record("deleted", deleted);
    span.record("skipped", skipped);
}

#[derive(Default, Debug)]
pub struct WordFormTemplate {
    pub english: String,
    pub xhosa: String,
    pub part_of_speech: Option<PartOfSpeech>,
    pub xhosa_tone_markings: String,
    pub infinitive: String,
    pub is_plural: bool,
    pub is_inchoative: bool,
    pub transitivity: Option<Transitivity>,
    pub followed_by: Option<ConjunctionFollowedBy>,
    pub noun_class: Option<NounClass>,
    pub note: String,
    pub is_informal: bool,
    pub examples: Vec<ExampleTemplate>,
    pub linked_words: Vec<LinkedWordTemplate>,
}

impl WordFormTemplate {
    #[instrument(name = "Fetch word form template", skip(db))]
    pub fn fetch_from_db(
        db: &impl UserAccessDb,
        existing: Option<u64>,
        suggested: Option<u64>,
    ) -> Option<Self> {
        match (existing, suggested) {
            (Some(existing), Some(suggestion)) => {
                let suggested_word = SuggestedWord::fetch_full(db, suggestion)?;
                let mut template = WordFormTemplate::from(suggested_word);
                template.examples.extend(
                    ExistingExample::fetch_all_for_word(db, existing)
                        .into_iter()
                        .map(Into::into),
                );
                template.linked_words.extend(
                    ExistingLinkedWord::fetch_all_for_word(db, existing)
                        .into_iter()
                        .map(Into::into),
                );
                Some(template)
            }
            (_, Some(suggestion)) => {
                let suggested_word = SuggestedWord::fetch_full(db, suggestion)?;
                Some(WordFormTemplate::from(suggested_word))
            }
            (Some(existing), None) => {
                let existing_word = ExistingWord::fetch_full(db, existing)?;
                Some(WordFormTemplate::from(existing_word))
            }
            _ => None,
        }
    }
}

impl From<SuggestedWord> for WordFormTemplate {
    fn from(w: SuggestedWord) -> Self {
        let this_id = w.this_id();
        WordFormTemplate {
            english: w.english.current().clone(),
            xhosa: w.xhosa.current().clone(),
            part_of_speech: Some(*w.part_of_speech.current()),
            xhosa_tone_markings: w.xhosa_tone_markings.current().clone(),
            infinitive: w.infinitive.current().clone(),
            is_plural: *w.is_plural.current(),
            is_inchoative: *w.is_inchoative.current(),
            transitivity: *w.transitivity.current(),
            followed_by: w.followed_by.current().clone(),
            noun_class: *w.noun_class.current(),
            note: w.note.current().clone(),
            is_informal: *w.is_informal.current(),
            examples: w.examples.into_iter().map(Into::into).collect(),
            linked_words: w
                .linked_words
                .into_iter()
                .map(|s| LinkedWordTemplate::from_suggested(s, this_id))
                .collect(),
        }
    }
}

impl From<ExistingWord> for WordFormTemplate {
    fn from(w: ExistingWord) -> Self {
        WordFormTemplate {
            english: w.english,
            xhosa: w.xhosa,
            part_of_speech: Some(w.part_of_speech),
            xhosa_tone_markings: w.xhosa_tone_markings,
            infinitive: w.infinitive,
            is_plural: w.is_plural,
            is_inchoative: w.is_inchoative,
            transitivity: w.transitivity,
            followed_by: w.followed_by,
            noun_class: w.noun_class,
            note: w.note,
            is_informal: w.is_informal,
            examples: w.examples.into_iter().map(Into::into).collect(),
            linked_words: w.linked_words.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ExampleTemplate {
    pub suggestion_id: Option<u64>,
    pub existing_id: Option<u64>,
    pub english: String,
    pub xhosa: String,
}

impl From<SuggestedExample> for ExampleTemplate {
    fn from(ex: SuggestedExample) -> Self {
        ExampleTemplate {
            suggestion_id: Some(ex.suggestion_id),
            existing_id: ex.existing_example_id,
            english: ex.english.current().clone(),
            xhosa: ex.xhosa.current().clone(),
        }
    }
}

impl From<ExistingExample> for ExampleTemplate {
    fn from(ex: ExistingExample) -> Self {
        ExampleTemplate {
            suggestion_id: None,
            existing_id: Some(ex.example_id),
            english: ex.english,
            xhosa: ex.xhosa,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LinkedWordTemplate {
    pub suggestion_id: Option<u64>,
    pub existing_id: Option<u64>,
    pub link_type: WordLinkType,
    pub other: WordHit,
}

impl LinkedWordTemplate {
    fn from_suggested(suggestion: SuggestedLinkedWord, this_id: WordOrSuggestionId) -> Self {
        LinkedWordTemplate {
            suggestion_id: Some(suggestion.suggestion_id),
            existing_id: suggestion.existing_linked_word_id,
            link_type: *suggestion.link_type.current(),
            other: suggestion.other(this_id).current().clone(),
        }
    }
}

impl From<ExistingLinkedWord> for LinkedWordTemplate {
    fn from(link: ExistingLinkedWord) -> Self {
        LinkedWordTemplate {
            suggestion_id: None,
            existing_id: Some(link.link_id),
            link_type: link.link_type,
            other: link.other,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct LinkedWordSubmission {
    suggestion_id: Option<u64>,
    existing_id: Option<u64>,
    link_type: WordLinkType,
    other: WordOrSuggestionId,
}

impl LinkedWordSubmission {
    fn has_any_changes(&self, o: &Option<ExistingLinkedWord>) -> bool {
        match o {
            Some(o) => {
                WordOrSuggestionId::existing(o.other.id) != self.other
                    || o.link_type != self.link_type
            }
            None => true,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct LinkedWordList(Vec<LinkedWordSubmission>);

impl<'de> Deserialize<'de> for LinkedWordList {
    fn deserialize<D>(deser: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Debug)]
        struct Raw {
            suggestion_id: Option<String>,
            existing_id: Option<String>,
            link_type: String,
            other: String,
        }

        #[derive(Deserialize, Debug)]
        struct Other {
            id: String,
            is_suggestion: String,
        }

        let raw = Vec::<Raw>::deserialize(deser)?;

        Ok(LinkedWordList(
            raw.into_iter()
                .filter_map(|raw| {
                    if raw.link_type.is_empty() {
                        return None;
                    }

                    let link_type = raw.link_type.parse().ok()?;
                    let suggestion_id = raw.suggestion_id.and_then(|x| x.parse::<u64>().ok());
                    let existing_id = raw.existing_id.and_then(|x| x.parse::<u64>().ok());

                    let other: Other = serde_json::from_str(&raw.other).ok()?;
                    let other_id = other.id.parse::<u64>().ok()?;
                    let other = if other.is_suggestion.parse::<bool>().ok()? {
                        WordOrSuggestionId::suggested(other_id)
                    } else {
                        WordOrSuggestionId::existing(other_id)
                    };

                    Some(LinkedWordSubmission {
                        suggestion_id,
                        existing_id,
                        link_type,
                        other,
                    })
                })
                .collect(),
        ))
    }
}

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
pub struct WordSubmission {
    pub suggestion_id: Option<u64>,
    pub existing_id: Option<u64>,

    // Used only in moderation page
    #[serde(default)]
    pub suggestion_anchor_ord: Option<u32>,

    pub english: String,
    pub xhosa: String,
    pub part_of_speech: PartOfSpeech,
    changes_summary: Option<String>,
    note: String,
    xhosa_tone_markings: String,
    infinitive: String,
    #[serde(default = "false_fn")]
    #[serde(deserialize_with = "deserialize_checkbox")]
    pub is_plural: bool,
    #[serde(default = "false_fn")]
    #[serde(deserialize_with = "deserialize_checkbox")]
    pub is_inchoative: bool,
    #[serde_as(as = "NoneAsEmptyString")]
    pub transitivity: Option<Transitivity>,
    #[serde_as(as = "NoneAsEmptyString")]
    pub followed_by: Option<ConjunctionFollowedBy>,
    pub noun_class: Option<NounClass>,

    #[serde(default = "false_fn")]
    #[serde(deserialize_with = "deserialize_checkbox")]
    pub is_informal: bool,

    #[serde(default)]
    examples: Vec<ExampleSubmission>,
    #[serde(default)]
    linked_words: LinkedWordList,
}

impl WordSubmission {
    fn has_any_changes_in_word(&self, o: &WordFormTemplate) -> bool {
        self.english != o.english
            || self.xhosa != o.xhosa
            || self.xhosa_tone_markings != o.xhosa_tone_markings
            || self.note != o.note
            || self.infinitive != o.infinitive
            || self.is_plural != o.is_plural
            || self.is_inchoative != o.is_inchoative
            || self.is_informal != o.is_informal
            || self.transitivity != o.transitivity
            || self.noun_class != o.noun_class
            || o.part_of_speech
                .map(|p| p != self.part_of_speech)
                .unwrap_or(true)
    }
}

#[derive(Deserialize, Clone, Debug)]
struct ExampleSubmission {
    suggestion_id: Option<u64>,
    existing_id: Option<u64>,
    english: String,
    xhosa: String,
}

impl ExampleSubmission {
    fn has_any_changes(&self, o: &Option<ExistingExample>) -> bool {
        match o {
            Some(o) => o.english != self.english || o.xhosa != self.xhosa,
            None => true,
        }
    }
}
