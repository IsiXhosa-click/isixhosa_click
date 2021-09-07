// TODO form validation for extra fields & make sure not empty str (or is empty str)
// TODO HTML sanitisation - allow markdown in text only, no html

use crate::language::{PartOfSpeech, WordLinkType};
use crate::search::WordHit;
use askama::Template;
use num_enum::TryFromPrimitive;

use rusqlite::{params, ToSql};
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::serde_as;
use std::fmt::{self, Debug, Display, Formatter};

use warp::{body, path, Filter, Rejection, Reply};

use crate::auth::UserAccessDb;
use crate::auth::{with_user_auth, Auth, DbBase, User};
use crate::database::existing::{ExistingExample, ExistingLinkedWord, ExistingWord};
use crate::database::suggestion::{SuggestedExample, SuggestedLinkedWord, SuggestedWord};
use crate::database::WordOrSuggestionId;
use crate::language::NounClassExt;
use crate::serialization::{deserialize_checkbox, false_fn, qs_form};
use isixhosa::noun::NounClass;
use rusqlite::types::{ToSqlOutput, Value};

#[derive(Template, Debug)]
#[template(path = "submit.askama.html")]
struct SubmitTemplate {
    auth: Auth,
    previous_success: Option<bool>,
    action: SubmitFormAction,
    word: WordFormTemplate,
}

#[derive(Deserialize, Debug, Copy, Clone)]
enum SubmitFormAction {
    EditSuggestion {
        suggestion_id: u64,
        existing_id: Option<u64>,
    },
    SubmitNewWord,
    EditExisting(u64),
}

impl Default for SubmitFormAction {
    fn default() -> Self {
        SubmitFormAction::SubmitNewWord
    }
}

impl Display for SubmitFormAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct WordId(pub u64);

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

        let raw = Vec::<Raw>::deserialize(deser)?;

        Ok(LinkedWordList(
            raw.into_iter()
                .filter_map(|raw| {
                    if raw.link_type.is_empty() {
                        return None;
                    }

                    let type_int = raw.link_type.parse::<u8>().ok()?;
                    let link_type = WordLinkType::try_from_primitive(type_int).ok()?;
                    let other = raw.other.parse::<u64>().ok().map(WordId)?;
                    let suggestion_id = raw.suggestion_id.and_then(|x| x.parse::<u64>().ok());
                    let existing_id = raw.existing_id.and_then(|x| x.parse::<u64>().ok());

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
    suggestion_id: Option<u64>,
    existing_id: Option<u64>,

    english: String,
    xhosa: String,
    part_of_speech: PartOfSpeech,
    note: String,
    xhosa_tone_markings: String,
    infinitive: String,
    #[serde(default = "false_fn")]
    #[serde(deserialize_with = "deserialize_checkbox")]
    is_plural: bool,
    noun_class: Option<NounClass>,

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

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
struct LinkedWordSubmission {
    suggestion_id: Option<u64>,
    existing_id: Option<u64>,
    link_type: WordLinkType,
    other: WordId,
}

impl LinkedWordSubmission {
    #[allow(clippy::suspicious_operation_groupings)] // false positive - self.other IS id
    fn has_any_changes(&self, o: &Option<ExistingLinkedWord>) -> bool {
        match o {
            Some(o) => o.other.id != self.other.0 || o.link_type != self.link_type,
            None => true,
        }
    }
}

pub fn submit(db: DbBase) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    // TODO handle unauthorized by redirect
    let submit_page = warp::get()
        .and(with_user_auth(db.clone()))
        .and(warp::any().map(|| None)) // previous_success is none
        .and(warp::any().map(SubmitFormAction::default))
        .and_then(submit_word_page);

    let submit_form = body::content_length_limit(4 * 1024)
        .and(with_user_auth(db.clone()))
        .and(qs_form())
        .and_then(submit_new_word_form);

    let failed_to_submit = warp::any()
        .and(with_user_auth(db))
        .and(warp::any().map(|| Some(false))) // previous_success is Some(false)
        .and(warp::any().map(SubmitFormAction::default))
        .and_then(submit_word_page);

    let submit_routes = submit_page.or(submit_form).or(failed_to_submit);

    warp::path("submit").and(path::end()).and(submit_routes).boxed()
}

pub async fn edit_suggestion_page(
    db: impl UserAccessDb,
    user: User,
    suggestion_id: u64,
) -> Result<impl Reply, Rejection> {
    let db_clone = db.clone();
    let existing_id = tokio::task::spawn_blocking(move || {
        SuggestedWord::fetch_existing_id_for_suggestion(&db_clone, suggestion_id)
    })
    .await
    .unwrap();

    submit_word_page(
        user,
        db,
        None,
        SubmitFormAction::EditSuggestion {
            suggestion_id,
            existing_id,
        },
    )
    .await
}

pub async fn edit_word_page(
    user: User,
    db: impl UserAccessDb,
    previous_success: Option<bool>,
    id: u64,
) -> Result<impl Reply, Rejection> {
    submit_word_page(
        user,
        db,
        previous_success,
        SubmitFormAction::EditExisting(id),
    )
    .await
}

#[derive(Default, Debug)]
struct WordFormTemplate {
    english: String,
    xhosa: String,
    part_of_speech: Option<PartOfSpeech>,
    xhosa_tone_markings: String,
    infinitive: String,
    is_plural: bool,
    noun_class: Option<NounClass>,
    note: String,
    examples: Vec<ExampleTemplate>,
    linked_words: Vec<LinkedWordTemplate>,
}

impl WordFormTemplate {
    fn fetch_from_db(
        db: &impl UserAccessDb,
        existing: Option<u64>,
        suggested: Option<u64>,
    ) -> Option<Self> {
        match (existing, suggested) {
            (Some(existing), Some(suggestion)) => {
                let suggested_word = SuggestedWord::fetch_full(db, suggestion)?;
                let mut template = WordFormTemplate::from(suggested_word);
                template.examples.extend(ExistingExample::fetch_all_for_word(db, existing).into_iter().map(Into::into));
                template.linked_words.extend(ExistingLinkedWord::fetch_all_for_word(db, existing).into_iter().map(Into::into));
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
            noun_class: *w.noun_class.current(),
            note: w.note.current().clone(),
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
            noun_class: w.noun_class,
            note: w.note,
            examples: w.examples.into_iter().map(Into::into).collect(),
            linked_words: w.linked_words.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ExampleTemplate {
    suggestion_id: Option<u64>,
    existing_id: Option<u64>,
    english: String,
    xhosa: String,
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
struct LinkedWordTemplate {
    suggestion_id: Option<u64>,
    existing_id: Option<u64>,
    link_type: WordLinkType,
    other: WordHit,
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

async fn submit_word_page(
    user: User,
    db: impl UserAccessDb,
    previous_success: Option<bool>,
    action: SubmitFormAction,
) -> Result<impl Reply, Rejection> {
    let db = db.clone();
    let word = tokio::task::spawn_blocking(move || match action {
        SubmitFormAction::EditSuggestion {
            suggestion_id,
            existing_id,
        } => WordFormTemplate::fetch_from_db(&db, existing_id, Some(suggestion_id))
            .unwrap_or_default(),
        SubmitFormAction::EditExisting(id) => {
            WordFormTemplate::fetch_from_db(&db, Some(id), None).unwrap_or_default()
        }
        SubmitFormAction::SubmitNewWord => WordFormTemplate::default(),
    })
    .await
    .unwrap();

    Ok(SubmitTemplate {
        auth: user.into(),
        previous_success,
        action,
        word,
    })
}

async fn submit_new_word_form(
    user: User,
    db: impl UserAccessDb,
    word: WordSubmission,
) -> Result<impl warp::Reply, Rejection> {
    submit_suggestion(word, &db).await;
    submit_word_page(user, db, Some(true), SubmitFormAction::SubmitNewWord).await
}

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

pub async fn suggest_word_deletion(word_id: WordId, db: &impl UserAccessDb) {
    const STATEMENT: &str =
        "INSERT INTO word_deletion_suggestions (word_id, reason) VALUES (?1, ?2);";

    let db = db.clone();

    tokio::task::spawn_blocking(move || {
        let conn = db.get().unwrap();
        conn.prepare(STATEMENT)
            .unwrap()
            .execute(params![word_id.0, "No reason given"])
            .unwrap();
    })
    .await
    .unwrap()
}

// TODO move to db module
pub async fn submit_suggestion(word: WordSubmission, db: &impl UserAccessDb) {
    const INSERT_SUGGESTION: &str = "
        INSERT INTO word_suggestions (
            suggestion_id, existing_word_id, changes_summary, english, xhosa,
            part_of_speech, xhosa_tone_markings, infinitive, is_plural, noun_class, note
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(suggestion_id) DO UPDATE SET
                existing_word_id = excluded.existing_word_id,
                changes_summary = excluded.changes_summary,
                english = excluded.english,
                xhosa = excluded.xhosa,
                part_of_speech = excluded.part_of_speech,
                xhosa_tone_markings = excluded.xhosa_tone_markings,
                infinitive = excluded.infinitive,
                is_plural = excluded.is_plural,
                noun_class = excluded.noun_class,
                note = excluded.note
            RETURNING suggestion_id;
        ";

    let db = db.clone();
    let mut w = word;

    tokio::task::spawn_blocking(move || {
        let conn = db.get().unwrap();

        let orig = WordFormTemplate::fetch_from_db(&db, w.existing_id, None).unwrap_or_default();
        let use_submitted = w.existing_id.is_none();

        // HACK(restioson): 255 is sentinel for "no noun class" as opposed to null which is noun class
        // not changed. It's bad I know but I don't have the energy for anything else, feel free to
        // submit a PR which implements a more principled solution and I will gladly merge it.
        let noun_class: ToSqlOutput<'static> = match w.noun_class {
            Some(class) if w.noun_class != orig.noun_class => {
                ToSqlOutput::Owned(Value::Integer(class as u8 as i64))
            }
            Some(_) => None::<u8>.to_sql().unwrap(),
            None => 255u8.to_sql().unwrap(),
        };

        let existing_id = w.existing_id;

        let changes_summary = if w.existing_id.is_none() {
            "Word added"
        } else {
            "Word edited"
        };

        let params = params![
            w.suggestion_id,
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
            noun_class,
            diff(w.note.clone(), &orig.note, use_submitted)
        ];

        let orig_suggestion = WordFormTemplate::fetch_from_db(&db, None, w.suggestion_id);

        let any_changes = match orig_suggestion {
            Some(orig_suggestion) => w.has_any_changes_in_word(&orig_suggestion),
            None => w.has_any_changes_in_word(&orig),
        };

        let suggested_word_id = if any_changes {
            let suggested_word_id: i64 = conn
                .prepare(INSERT_SUGGESTION)
                .unwrap()
                .query_row(params, |row| row.get("suggestion_id"))
                .unwrap();
            Some(suggested_word_id).filter(|_| existing_id.is_none())
        } else {
            w.suggestion_id.map(|id| id as i64)
        };

        process_linked_words(&mut w, &db, suggested_word_id);
        process_examples(&mut w, &db, suggested_word_id);
    })
    .await
    .unwrap();
}

fn process_linked_words(
    w: &mut WordSubmission,
    db: &impl UserAccessDb,
    suggested_word_id: Option<i64>,
) {
    const INSERT_LINKED_WORD_SUGGESTION: &str = "
        INSERT INTO linked_word_suggestions (
            suggestion_id, existing_linked_word_id, changes_summary, suggested_word_id,
            link_type, first_existing_word_id, second_existing_word_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(suggestion_id) DO UPDATE SET
                changes_summary = excluded.changes_summary,
                suggested_word_id = excluded.suggested_word_id,
                link_type = excluded.link_type,
                first_existing_word_id = excluded.first_existing_word_id,
                second_existing_word_id = excluded.second_existing_word_id;
        ";

    const DELETE_LINKED_WORD_SUGGESTION: &str =
        "DELETE FROM linked_word_suggestions WHERE suggestion_id = ?1;";

    const SUGGEST_LINKED_WORD_DELETION: &str =
        "INSERT INTO linked_word_deletion_suggestions (linked_word_id, reason) VALUES (?1, ?2)";

    let use_submitted = w.existing_id.is_none() && w.suggestion_id.is_none();

    let conn = db.get().unwrap();
    let mut upsert_suggested_link = conn.prepare(INSERT_LINKED_WORD_SUGGESTION).unwrap();
    let mut delete_suggested_link = conn.prepare(DELETE_LINKED_WORD_SUGGESTION).unwrap();
    let mut suggest_link_deletion = conn.prepare(SUGGEST_LINKED_WORD_DELETION).unwrap();

    let existing_word_id = w.existing_id;
    let mut maybe_insert_link = |new: LinkedWordSubmission, old: Option<ExistingLinkedWord>| {
        if !new.has_any_changes(&old) {
            return;
        }

        upsert_suggested_link
            .execute(params![
                new.suggestion_id,
                new.existing_id,
                "Linked word added",
                suggested_word_id,
                diff_opt(
                    new.link_type,
                    &old.as_ref().map(|o| o.link_type),
                    use_submitted
                ),
                diff_opt(
                    new.other.0,
                    &old.as_ref().map(|o| o.other.id),
                    use_submitted
                ),
                existing_word_id,
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
                    let old = new
                        .existing_id
                        .and_then(|id| ExistingLinkedWord::get(db, id, existing_word_id.unwrap()));
                    maybe_insert_link(new, old);
                } else {
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
                    suggest_link_deletion
                        .execute(params![prev.link_id, "No reason given"])
                        .unwrap();
                }
            }
        }
        // Brand new word submission
        (None, None) => {}
    }

    // Newly added linked words
    for new in &w.linked_words.0 {
        upsert_suggested_link
            .execute(params![
                new.suggestion_id,
                new.existing_id,
                "Linked word added",
                suggested_word_id,
                new.link_type,
                new.other.0.to_string(),
                w.existing_id,
            ])
            .unwrap();
    }
}

fn process_examples(
    w: &mut WordSubmission,
    db: &impl UserAccessDb,
    suggested_word_id: Option<i64>,
) {
    const INSERT_EXAMPLE_SUGGESTION: &str = "
        INSERT INTO example_suggestions (
            suggestion_id, existing_example_id, changes_summary, suggested_word_id,
            existing_word_id, english, xhosa
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(suggestion_id) DO UPDATE SET
                changes_summary = excluded.changes_summary,
                suggested_word_id = excluded.suggested_word_id,
                existing_word_id = excluded.existing_word_id,
                english = excluded.english,
                xhosa = excluded.xhosa;
        ";

    const DELETE_EXAMPLE_SUGGESTION: &str =
        "DELETE FROM example_suggestions WHERE suggestion_id = ?1;";

    const SUGGEST_EXAMPLE_DELETION: &str =
        "INSERT INTO example_deletion_suggestions (example_id, reason) VALUES (?1, ?2);";

    let conn = db.get().unwrap();
    let mut upsert_example = conn.prepare(INSERT_EXAMPLE_SUGGESTION).unwrap();
    let mut delete_suggested_example = conn.prepare(DELETE_EXAMPLE_SUGGESTION).unwrap();
    let mut suggest_example_deletion = conn.prepare(SUGGEST_EXAMPLE_DELETION).unwrap();

    let use_submitted = w.existing_id.is_none() && w.suggestion_id.is_none();
    let existing_id = w.existing_id;
    let examples = &mut w.examples;
    let mut maybe_insert_example = |new: ExampleSubmission, old: Option<ExistingExample>| {
        if !new.has_any_changes(&old) {
            return;
        }

        upsert_example
            .execute(params![
                new.suggestion_id,
                new.existing_id,
                "Example edited",
                suggested_word_id,
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
                    let old = new.existing_id.and_then(|id| ExistingExample::get(db, id));
                    maybe_insert_example(new, old);
                } else {
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
                        suggest_example_deletion
                            .execute(params![prev.example_id, "No reason given"])
                            .unwrap();
                    }
                }
            }
        }
        (None, None) => {}
    }

    for new in &w.examples {
        if new.english.is_empty() && new.xhosa.is_empty() {
            continue;
        }

        upsert_example
            .execute(params![
                new.suggestion_id,
                new.existing_id,
                "Example added",
                suggested_word_id,
                w.existing_id,
                new.english,
                new.xhosa
            ])
            .unwrap();
    }
}
