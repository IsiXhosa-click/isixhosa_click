// TODO form validation for extra fields & make sure not empty str (or is empty str)
// TODO HTML sanitisation - allow markdown in text only, no html
// TODO handle multiple examples

use crate::language::{NounClass, PartOfSpeech, WordLinkType};
use crate::typesense::WordHit;
use askama::Template;
use num_enum::TryFromPrimitive;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use reqwest::header::CONTENT_TYPE;
use rusqlite::{params, ToSql};
use serde::de::{DeserializeOwned, Error};
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::serde_as;
use std::fmt::Debug;
use warp::hyper::body::Bytes;
use warp::{body, path, Buf, Filter, Rejection, Reply};

use crate::database::suggestion::{
    get_examples_for_suggestion, get_full_suggested_word, get_linked_words_for_suggestion,
    SuggestedExample, SuggestedLinkedWord, SuggestedWord,
};

#[derive(Template, Debug)]
#[template(path = "submit.html")]
struct SubmitTemplate {
    previous_success: Option<bool>,
    route: &'static str,
    word: WordFormTemplate,
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
struct WordId(i64);

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
enum FormOption<T> {
    Some(T),
    None,
}

impl<T> From<FormOption<T>> for Option<T> {
    fn from(form: FormOption<T>) -> Option<T> {
        match form {
            FormOption::Some(v) => Some(v),
            FormOption::None => None,
        }
    }
}

impl<'de, T: Deserialize<'de> + TryFromPrimitive<Primitive = u8>> Deserialize<'de>
    for FormOption<T>
{
    fn deserialize<D>(deser: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let str = String::deserialize(deser)?;

        if str.is_empty() {
            Ok(FormOption::None)
        } else {
            let int = str
                .parse::<u8>()
                .map_err(|_| Error::custom("Invalid integer format"))?;
            let inner = T::try_from_primitive(int)
                .map_err(|_| Error::custom("Invalid integer discriminator"))?;
            Ok(FormOption::Some(inner))
        }
    }
}

#[derive(Clone, Debug, Default)]
struct LinkedWordList(Vec<LinkedWord>);

impl<'de> Deserialize<'de> for LinkedWordList {
    fn deserialize<D>(deser: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Debug)]
        struct Raw {
            suggestion_id: Option<String>,
            link_type: String,
            other: String,
        }

        let raw = dbg!(Vec::<Raw>::deserialize(deser))?;

        Ok(LinkedWordList(
            raw.into_iter()
                .filter_map(|raw| {
                    if dbg!(raw.link_type.is_empty()) {
                        return None;
                    }

                    let type_int = dbg!(raw.link_type).parse::<u8>().ok()?;
                    let link_type = dbg!(WordLinkType::try_from_primitive(type_int)).ok()?;
                    let other = dbg!(raw.other).parse::<i64>().ok().map(WordId)?;
                    let suggestion_id = dbg!(raw.suggestion_id).and_then(|x| x.parse::<i64>().ok());

                    Some(dbg!(LinkedWord {
                        suggestion_id,
                        link_type,
                        other
                    }))
                })
                .collect(),
        ))
    }
}

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
pub struct WordSubmission {
    english: String,
    xhosa: String,
    part_of_speech: PartOfSpeech,
    suggestion_id: Option<i64>,

    xhosa_tone_markings: String,
    infinitive: String,
    #[serde(default = "false_fn")]
    #[serde(deserialize_with = "deserialize_checkbox")]
    is_plural: bool,
    noun_class: FormOption<NounClass>,
    #[serde(default)]
    examples: Vec<Example>,
    #[serde(default)]
    linked_words: LinkedWordList,
    note: String,
}

#[derive(Deserialize, Clone, Debug)]
struct Example {
    suggestion_id: Option<i64>,
    english: String,
    xhosa: String,
}

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
struct LinkedWord {
    suggestion_id: Option<i64>,
    link_type: WordLinkType,
    other: WordId,
}

fn false_fn() -> bool {
    false
}

fn deserialize_checkbox<'de, D>(deser: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match String::deserialize(deser)? {
        str if str.to_lowercase() == "on" || str.to_lowercase() == "true" => Ok(true),
        str if str.to_lowercase() == "off" || str.to_lowercase() == "false" => Ok(false),
        other => Err(serde::de::Error::custom(format!(
            "Invalid checkbox bool string {}",
            other
        ))),
    }
}

fn to_bytes<B: Buf>(mut b: B) -> Bytes {
    b.copy_to_bytes(b.remaining())
}

pub fn qs_form<T: DeserializeOwned + Send>() -> impl Filter<Extract = (T,), Error = Rejection> + Copy
{
    warp::header::exact(CONTENT_TYPE.as_ref(), "application/x-www-form-urlencoded")
        .and(warp::body::aggregate())
        .map(to_bytes)
        .and_then(|bytes: Bytes| async move {
            serde_qs::Config::new(5, false)
                .deserialize_bytes(&bytes)
                .map_err(|err| {
                    #[derive(Debug)]
                    struct DeserErr(serde_qs::Error);

                    impl warp::reject::Reject for DeserErr {}

                    dbg!(&err);

                    warp::reject::custom(DeserErr(err))
                })
        })
}

#[derive(Debug, Default)]
struct SubmitParams {
    suggestion: Option<i64>,
}

pub fn submit(
    db: Pool<SqliteConnectionManager>,
) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    let db = warp::any().map(move || db.clone());

    let submit_page = warp::get()
        .and(db.clone())
        .and(warp::any().map(|| None)) // previous_success is none
        .and(warp::any().map(|| "/submit"))
        .and(warp::any().map(SubmitParams::default))
        .and_then(submit_word_page);

    let submit_form = body::content_length_limit(4 * 1024)
        .and(qs_form())
        .and(db.clone())
        .and_then(submit_word_form);

    let failed_to_submit = warp::any()
        .and(db)
        .and(warp::any().map(|| Some(false))) // previous_success is Some(false)
        .and(warp::any().map(|| "/submit"))
        .and(warp::any().map(SubmitParams::default))
        .and_then(submit_word_page);

    let submit_routes = submit_page.or(submit_form).or(failed_to_submit);

    // TODO or failed
    warp::path("submit").and(path::end()).and(submit_routes)
}

pub async fn edit_suggestion_page(
    db: Pool<SqliteConnectionManager>,
    id: i64,
) -> Result<impl Reply, Rejection> {
    submit_word_page(
        db,
        None,
        "/accept/edit",
        SubmitParams {
            suggestion: Some(id),
        },
    )
    .await
}

#[derive(Default, Debug)]
struct WordFormTemplate {
    suggestion_id: Option<i64>,
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

impl From<SuggestedWord> for WordFormTemplate {
    fn from(w: SuggestedWord) -> Self {
        WordFormTemplate {
            suggestion_id: Some(w.suggestion_id),
            english: w.english.current().clone(),
            xhosa: w.xhosa.current().clone(),
            part_of_speech: Some(*w.part_of_speech.current()),
            xhosa_tone_markings: w.xhosa_tone_markings.current().clone(),
            infinitive: w.infinitive.current().clone(),
            is_plural: *w.is_plural.current(),
            noun_class: *w.noun_class.current(),
            note: w.note.current().clone(),
            examples: w.examples.into_iter().map(Into::into).collect(),
            linked_words: w.linked_words.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ExampleTemplate {
    suggestion_id: i64,
    english: String,
    xhosa: String,
}

impl From<SuggestedExample> for ExampleTemplate {
    fn from(ex: SuggestedExample) -> Self {
        ExampleTemplate {
            suggestion_id: ex.suggestion_id,
            english: ex.english.current().clone(),
            xhosa: ex.xhosa.current().clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct LinkedWordTemplate {
    suggestion_id: i64,
    link_type: WordLinkType,
    other: WordHit,
}

impl From<SuggestedLinkedWord> for LinkedWordTemplate {
    fn from(suggestion: SuggestedLinkedWord) -> Self {
        LinkedWordTemplate {
            suggestion_id: suggestion.suggestion_id,
            link_type: *suggestion.link_type.current(),
            other: suggestion.other,
        }
    }
}

async fn submit_word_page(
    db: Pool<SqliteConnectionManager>,
    previous_success: Option<bool>,
    route: &'static str,
    params: SubmitParams,
) -> Result<impl Reply, Rejection> {
    let word = if let Some(id) = params.suggestion {
        tokio::task::spawn_blocking(move || {
            // TODO handle examples and linked words
            let suggested_word = get_full_suggested_word(db.clone(), id)?;
            Some(WordFormTemplate::from(suggested_word))
        })
        .await
        .unwrap()
        .unwrap_or_default()
    } else {
        WordFormTemplate::default()
    };

    Ok(dbg!(SubmitTemplate {
        previous_success,
        route,
        word,
    }))
}

async fn submit_word_form(
    word: WordSubmission,
    db: Pool<SqliteConnectionManager>,
) -> Result<impl warp::Reply, Rejection> {
    submit_suggestion(word, db.clone()).await;
    submit_word_page(db, Some(true), "/submit", SubmitParams::default()).await
}

pub async fn submit_suggestion(word: WordSubmission, db: Pool<SqliteConnectionManager>) {
    const INSERT_SUGGESTION: &str = "
        INSERT INTO word_suggestions (
            suggestion_id, changes_summary, deletion, english, xhosa, part_of_speech,
            xhosa_tone_markings, infinitive, is_plural, noun_class, note
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(suggestion_id) DO UPDATE SET
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

    const INSERT_LINKED_WORD_SUGGESTION: &str = "
        INSERT INTO linked_word_suggestions (
            suggestion_id, changes_summary, deletion, suggested_word_id, link_type, first_existing_word_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(suggestion_id) DO UPDATE SET
                changes_summary = excluded.changes_summary,
                suggested_word_id = excluded.suggested_word_id,
                link_type = excluded.link_type,
                first_existing_word_id = excluded.first_existing_word_id;
        ";

    const DELETE_LINKED_WORD_SUGGESTION: &str =
        "DELETE FROM linked_word_suggestions WHERE suggestion_id = ?1;";

    const INSERT_EXAMPLE_SUGGESTION: &str = "
        INSERT INTO example_suggestions (
            suggestion_id, changes_summary, deletion, suggested_word_id, english, xhosa
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(suggestion_id) DO UPDATE SET
                changes_summary = excluded.changes_summary,
                suggested_word_id = excluded.suggested_word_id,
                english = excluded.english,
                xhosa = excluded.xhosa;
        ";

    const DELETE_EXAMPLE_SUGGESTION: &str =
        "DELETE FROM example_suggestions WHERE suggestion_id = ?1;";

    // TODO support update (upsert)

    let _db_clone = db.clone();
    let mut w = word;

    dbg!(&w);

    tokio::task::spawn_blocking(move || {
        let conn = db.get().unwrap();

        // HACK(restioson): 255 is sentinel for "no noun class" as opposed to null which is noun class
        // not changed. It's bad I know but I don't have the energy for anything else, feel free to
        // submit a PR which implements a more principled solution and I will gladly merge it.
        let noun_class: Option<NounClass> = w.noun_class.into();
        let no_noun_class = 255u8.to_sql().unwrap();
        let noun_class = if noun_class.is_some() {
            noun_class.to_sql().unwrap()
        } else {
            no_noun_class
        };

        let params = params![
            w.suggestion_id,
            "Word added",
            false,
            w.english,
            w.xhosa,
            w.part_of_speech,
            w.xhosa_tone_markings,
            w.infinitive,
            w.is_plural,
            noun_class,
            w.note
        ];

        let suggestion_id: i64 = conn
            .prepare(INSERT_SUGGESTION)
            .unwrap()
            .query_row(params, |row| row.get("suggestion_id"))
            .unwrap();

        let mut upsert_link = conn.prepare(INSERT_LINKED_WORD_SUGGESTION).unwrap();
        let mut delete_link = conn.prepare(DELETE_LINKED_WORD_SUGGESTION).unwrap();
        let prev_linked = get_linked_words_for_suggestion(db.clone(), suggestion_id);

        for prev in prev_linked {
            dbg!(&prev);
            if let Some(i) = w
                .linked_words
                .0
                .iter()
                .position(|new| new.suggestion_id == Some(prev.suggestion_id))
            {
                let new = w.linked_words.0.remove(i);
                dbg!("Update", &new);

                upsert_link
                    .execute(params![
                        new.suggestion_id,
                        "Linked word added",
                        false,
                        suggestion_id,
                        new.link_type,
                        new.other.0.to_string()
                    ])
                    .unwrap();
            } else {
                delete_link.execute(params![prev.suggestion_id]).unwrap();
            }
        }

        for new in w.linked_words.0 {
            dbg!("Insert", &new);
            upsert_link
                .execute(params![
                    new.suggestion_id,
                    "Linked word added",
                    false,
                    suggestion_id,
                    new.link_type,
                    new.other.0.to_string()
                ])
                .unwrap();
        }

        let mut upsert_example = conn.prepare(INSERT_EXAMPLE_SUGGESTION).unwrap();
        let mut delete_example = conn.prepare(DELETE_EXAMPLE_SUGGESTION).unwrap();
        let prev_examples = get_examples_for_suggestion(db.clone(), suggestion_id);

        for prev in prev_examples {
            dbg!(&prev);
            if let Some(i) = w
                .examples
                .iter()
                .position(|new| new.suggestion_id == Some(prev.suggestion_id))
            {
                let new = w.examples.remove(i);

                if new.english.is_empty() && new.xhosa.is_empty() {
                    delete_example.execute(params![prev.suggestion_id]).unwrap();
                    continue;
                }

                dbg!("Update", &new);
                upsert_example
                    .execute(params![
                        new.suggestion_id,
                        "Example added",
                        false,
                        suggestion_id,
                        new.english,
                        new.xhosa
                    ])
                    .unwrap();
            } else {
                delete_example.execute(params![prev.suggestion_id]).unwrap();
            }
        }

        for new in w.examples {
            if new.english.is_empty() && new.xhosa.is_empty() {
                continue;
            }

            dbg!("Insert", &new);
            upsert_example
                .execute(params![
                    new.suggestion_id,
                    "Example added",
                    false,
                    suggestion_id,
                    new.english,
                    new.xhosa
                ])
                .unwrap();
        }
    })
    .await
    .unwrap();
}
