// TODO form validation for extra fields
// TODO HTML sanitisation - allow markdown in text only, no html
// TODO handle multiple examples

use crate::language::{NounClass, PartOfSpeech};
use crate::typesense::{TypesenseClient, WordDocument};
use askama::Template;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Deserializer, Serialize};
use warp::http::Uri;
use warp::reject::Reject;
use warp::{body, Filter, Rejection, Reply};

#[derive(Template, Deserialize)]
#[template(path = "submit.html")]
struct SubmitTemplate {
    success: Option<bool>,
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
struct WordId(i32);

#[derive(Serialize, Deserialize, Clone, Debug)]
struct WordSubmission {
    english: String,
    xhosa: String,
    part_of_speech: PartOfSpeech,

    xhosa_tone_markings: Option<String>,
    infinitive: Option<String>,
    #[serde(default = "false_fn")]
    #[serde(deserialize_with = "deserialize_checkbox")]
    is_plural: bool,
    other_plurality_form: Option<WordId>,
    noun_class: Option<NounClass>,
    example_english: Option<String>,
    example_xhosa: Option<String>,
    note: Option<String>,
}

fn false_fn() -> bool {
    false
}

fn deserialize_checkbox<'de, D>(deser: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match String::deserialize(deser)? {
        str if str.to_lowercase() == "on" => Ok(true),
        other => Err(serde::de::Error::custom(format!(
            "Invalid checkbox bool string {}",
            other
        ))),
    }
}

pub fn submit(
    db: Pool<SqliteConnectionManager>,
    typesense: TypesenseClient,
) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    let db = warp::any().map(move || db.clone());
    let typesense = warp::any().map(move || typesense.clone());
    warp::path("submit").and(warp::path::end()).and(
        (warp::get().and(warp::query::<SubmitTemplate>()))
        .or(body::content_length_limit(2 * 1024)
            .and(body::form())
            .and(db)
            .and(typesense)
            .and_then(submit_word_form))
        .or(warp::any().map(|| warp::redirect("/submit?success=true".parse::<Uri>().unwrap()))),
    )
}

async fn submit_word_form(
    word: WordSubmission,
    db: Pool<SqliteConnectionManager>,
    typesense: TypesenseClient,
) -> Result<impl warp::Reply, Rejection> {
    const INSERT: &str = "
    INSERT INTO words (
        english, xhosa, part_of_speech,
        xhosa_tone_markings, infinitive, is_plural, noun_class,
        example_english, example_xhosa, note
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);";

    let w = word.clone();

    let id = tokio::task::spawn_blocking(move || {
        let conn = db.get().unwrap();

        conn.prepare(INSERT)
            .unwrap()
            .execute(params![
                w.english,
                w.xhosa,
                w.part_of_speech,
                w.xhosa_tone_markings,
                w.infinitive,
                w.is_plural,
                w.noun_class,
                w.example_english,
                w.example_xhosa,
                w.note
            ])
            .unwrap();
        conn.last_insert_rowid()
    })
    .await
    .unwrap();

    #[derive(Debug, Copy, Clone)]
    struct TypesenseError;
    impl Reject for TypesenseError {}

    typesense
        .add_word(WordDocument {
            id: id.to_string(),
            english: word.english,
            xhosa: word.xhosa,
            part_of_speech: word.part_of_speech,
            is_plural: word.is_plural,
            noun_class: word.noun_class,
        })
        .await
        .map_err(|e| {
            eprintln!("Error adding a word to typesense: {:#?}", e);
            warp::reject::custom(TypesenseError)
        })?;

    Ok(warp::redirect(
        "/submit?success=true".parse::<Uri>().unwrap(),
    ))
}
