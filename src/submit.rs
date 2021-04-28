// TODO form validation for extra fields & make sure not empty str (or is empty str)
// TODO HTML sanitisation - allow markdown in text only, no html
// TODO handle multiple examples

use crate::language::{NounClass, PartOfSpeech, WordLinkType};
use crate::typesense::{TypesenseClient, WordDocument, WordHit};
use askama::Template;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, ToSql};
use serde::{Deserialize, Deserializer, Serialize};
use warp::http::Uri;
use warp::reject::Reject;
use warp::{body, Filter, Rejection, Reply, Buf, path};
use serde::de::{DeserializeOwned, Error};
use reqwest::header::CONTENT_TYPE;
use warp::hyper::body::Bytes;
use serde_with::{serde_as, DefaultOnError};
use serde_with::rust::string_empty_as_none;
use std::fmt::Debug;
use num_enum::TryFromPrimitive;
use rusqlite::types::ToSqlOutput;
use crate::accept::{SuggestedWord, get_suggested_word_alone, MaybeEdited, SuggestedExample, SuggestedLinkedWord, get_full_suggested_word};
use crate::database::get_word_hit_from_db;

#[derive(Template, Debug)]
#[template(path = "submit.html")]
struct SubmitTemplate {
    previous_success: Option<bool>,
    route: String,
    word: WordFormTemplate,
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
struct WordId(i32);

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
enum FormOption<T> {
    Some(T),
    None,
}

impl<T> Into<Option<T>> for FormOption<T> {
    fn into(self) -> Option<T> {
        match self {
            FormOption::Some(v) => Some(v),
            FormOption::None => None,
        }
    }
}

impl<'de, T: Deserialize<'de> + TryFromPrimitive<Primitive = u8>> Deserialize<'de> for FormOption<T> {
    fn deserialize<D>(deser: D) -> Result<Self, D::Error> where
        D: Deserializer<'de>
    {
        let str = String::deserialize(deser)?;

        if str.is_empty() {
            Ok(FormOption::None)
        } else {
            let int = str.parse::<u8>().map_err(|_| Error::custom("Invalid integer format"))?;
            let inner = T::try_from_primitive(int).map_err(|_| Error::custom("Invalid integer discriminator"))?;
            Ok(FormOption::Some(inner))
        }
    }
}

#[derive(Clone, Debug, Default)]
struct LinkedWordList(Vec<LinkedWord>);

impl<'de> Deserialize<'de> for LinkedWordList {
    fn deserialize<D>(deser: D) -> Result<Self, D::Error> where
        D: Deserializer<'de>
    {
        #[derive(Deserialize)]
        struct Raw {
            link_type: String,
            other: String,
        }

        let raw = Vec::<Raw>::deserialize(deser)?;

        Ok(LinkedWordList(raw.into_iter().filter_map(|raw| {
            if raw.link_type.is_empty() {
                return None;
            }

            let type_int = raw.link_type.parse::<u8>().ok()?;
            let link_type = WordLinkType::try_from_primitive(type_int).ok()?;
            let other = raw.other.parse::<i32>().ok().map(WordId)?;

            Some(LinkedWord { link_type, other })
        }).collect()))
    }
}

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
struct WordSubmission {
    english: String,
    xhosa: String,
    part_of_speech: PartOfSpeech,

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
    english: String,
    xhosa: String,
}

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
struct LinkedWord {
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

fn qs_form<T: DeserializeOwned + Send>() -> impl Filter<Extract = (T,), Error = Rejection> + Copy {
    warp::header::exact(CONTENT_TYPE.as_ref(), "application/x-www-form-urlencoded")
        .and(warp::body::aggregate())
        .map(to_bytes)
        .and_then(|bytes: Bytes| async move {
            serde_qs::Config::new(5, false).deserialize_bytes(&bytes).map_err(|err| {
                #[derive(Debug)]
                struct DeserErr(serde_qs::Error);

                impl warp::reject::Reject for DeserErr {}

                dbg!(&err);

                warp::reject::custom(DeserErr(err))
            })
        })
}

#[derive(Debug)]
struct SubmitParams {
    suggestion: Option<i32>,
    route: String,
}

impl Default for SubmitParams {
    fn default() -> Self {
        SubmitParams {
            suggestion: None,
            route: "/submit".to_string(),
        }
    }
}

pub fn submit(
    db: Pool<SqliteConnectionManager>,
    typesense: TypesenseClient,
) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    let db = warp::any().map(move || db.clone());
    let typesense = warp::any().map(move || typesense.clone());

    let submit_page = warp::get()
        .and(db.clone())
        .and(warp::any().map(|| None)) // previous_success is none
        .and(warp::any().map(|| SubmitParams::default()))
        .and_then(submit_word_page);

    let submit_form = body::content_length_limit(2 * 1024)
        .and(qs_form())
        .and(db.clone())
        .and(typesense)
        .and_then(submit_word_form);

    let failed_to_submit = warp::any()
        .and(db)
        .and(warp::any().map(|| Some(false))) // previous_success is Some(false)
        .and(warp::any().map(|| SubmitParams::default()))
        .and_then(submit_word_page);

    let submit_routes = submit_page.or(submit_form).or(failed_to_submit);

    // TODO or failed
    warp::path("submit").and(path::end()).and(submit_routes)
}

pub async fn edit_suggestion(db: Pool<SqliteConnectionManager>, id: i32, route: String) -> Result<impl Reply, Rejection> {
    submit_word_page(db, None, SubmitParams { suggestion: Some(id), route }).await
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
    fn from_suggested(w: SuggestedWord, db: Pool<SqliteConnectionManager>) -> Self {
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
            linked_words: w.linked_words.into_iter().map(move |s| LinkedWordTemplate::from_suggestion(s, db.clone())).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ExampleTemplate {
    english: String,
    xhosa: String,
}

impl From<SuggestedExample> for ExampleTemplate {
    fn from(ex: SuggestedExample) -> Self {
        ExampleTemplate {
            english: ex.english.current().clone(),
            xhosa: ex.xhosa.current().clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct LinkedWordTemplate {
    link_type: WordLinkType,
    other: WordHit,
}

impl LinkedWordTemplate {
    fn from_suggestion(suggestion: SuggestedLinkedWord, db: Pool<SqliteConnectionManager>) -> Self {
        let other = get_word_hit_from_db(db, suggestion.first_existing_word_id).unwrap();

        LinkedWordTemplate {
            link_type: *suggestion.link_type.current(),
            other,
        }
    }
}

async fn submit_word_page(
    db: Pool<SqliteConnectionManager>,
    previous_success: Option<bool>,
    params: SubmitParams
) -> Result<impl Reply, Rejection> {
    let word = if let Some(id) = params.suggestion {
        tokio::task::spawn_blocking(move || {
            // TODO handle examples and linked words
            let suggested_word = get_full_suggested_word(db.clone(), id)?;
            Some(WordFormTemplate::from_suggested(suggested_word, db))
        }).await.unwrap().unwrap_or_default()
    } else {
        WordFormTemplate::default()
    };

    Ok(dbg!(SubmitTemplate {
        previous_success,
        route: params.route,
        word,
    }))
}

async fn submit_word_form(
    word: WordSubmission,
    db: Pool<SqliteConnectionManager>,
    typesense: TypesenseClient,
) -> Result<impl warp::Reply, Rejection> {
    const INSERT_SUGGESTION: &str = "
        INSERT INTO word_suggestions (
            changes_summary, deletion, english, xhosa, part_of_speech,
            xhosa_tone_markings, infinitive, is_plural, noun_class, note
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

    const INSERT_LINKED_WORD_SUGGESTION: &str = "
        INSERT INTO linked_word_suggestions (
            changes_summary, deletion, suggested_word_id, link_type, first_existing_word_id
        ) VALUES (?1, ?2, ?3, ?4, ?5)";

    const INSERT_EXAMPLE_SUGGESTION: &str = "
        INSERT INTO example_suggestions (
            changes_summary, deletion, suggested_word_id, english, xhosa
        ) VALUES (?1, ?2, ?3, ?4, ?5)";

    let db_clone = db.clone();
    let w = word.clone();

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

        conn.prepare(INSERT_SUGGESTION)
            .unwrap()
            .execute(params![
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
            ])
            .unwrap();

        let suggestion_id = conn.last_insert_rowid();

        for l in w.linked_words.0 {
            conn.prepare(INSERT_LINKED_WORD_SUGGESTION)
                .unwrap()
                .execute(params!["Linked word added", false, suggestion_id, l.link_type, l.other.0.to_string()])
                .unwrap();
        }

        for e in w.examples {
            conn.prepare(INSERT_EXAMPLE_SUGGESTION)
                .unwrap()
                .execute(params!["Example added", false, suggestion_id, e.english, e.xhosa])
                .unwrap();
        }
    })
    .await
    .unwrap();

    submit_word_page(db_clone, Some(true), SubmitParams::default()).await
}

// TODO do something with this
// async fn add_word(typesense: TypesenseClient, id: WordId, english: String, xhosa: String, part: PartOfSpeech, plural: bool, class: Option<NounClass>) {
//     #[derive(Debug, Copy, Clone)]
//     struct TypesenseError;
//     impl Reject for TypesenseError {}
//
//     typesense
//         .add_word(WordDocument {
//             id: id.0.to_string(),
//             english,
//             xhosa,
//             part_of_speech: part,
//             is_plural: plural,
//             noun_class: class,
//         })
//         .await
//         .map_err(|e| {
//             eprintln!("Error adding a word to typesense: {:#?}", e);
//             warp::reject::custom(TypesenseError)
//         })?;
// }
