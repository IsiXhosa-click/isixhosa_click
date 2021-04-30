use crate::language::{NounClass, PartOfSpeech, SerializeDisplay};
use arcstr::ArcStr;
use askama::Template;
use itertools::Itertools;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Method;
use rusqlite::params;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::task;

const TYPESENSE_SCHEMA: &str = include_str!("typesense_schema.json");
const IMPORT_DOCUMENTS_LIMIT: u8 = 40;

#[derive(Clone, Debug)]
pub struct TypesenseClient {
    pub api_key: ArcStr,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Collection {
    pub num_documents: u64,
    pub name: String,
    pub fields: Vec<CollectionField>,
    pub default_sorting_field: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateCollection {
    pub name: String,
    pub fields: Vec<CollectionField>,
    pub default_sorting_field: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CollectionField {
    pub name: String,
    #[serde(rename = "type")]
    pub typ: FieldType,
    #[serde(default = "false_fn")]
    pub optional: bool,
    #[serde(default = "false_fn")]
    pub facet: bool,
}

fn false_fn() -> bool {
    false
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Int64,
    Int32,
    Float,
    String,
    Bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WordDocument {
    pub id: String,
    pub english: String,
    pub xhosa: String,
    pub part_of_speech: PartOfSpeech,
    pub is_plural: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default = "option_none")]
    pub noun_class: Option<NounClass>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Template, Default)]
#[template(path = "search.html")]
pub struct ShortWordSearchResults {
    hits: Vec<ShortWordSearchHit>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ShortWordSearchHit {
    document: WordHit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WordHit {
    pub id: String,
    pub english: String,
    pub xhosa: String,
    pub part_of_speech: SerializeDisplay<PartOfSpeech>,
    pub is_plural: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default = "option_none")]
    pub noun_class: Option<SerializeDisplay<NounClass>>,
}

impl From<WordDocument> for WordHit {
    fn from(d: WordDocument) -> Self {
        WordHit {
            id: d.id,
            english: d.english,
            xhosa: d.xhosa,
            part_of_speech: SerializeDisplay(d.part_of_speech),
            is_plural: d.is_plural,
            noun_class: d.noun_class.map(SerializeDisplay),
        }
    }
}

fn option_none<T>() -> Option<T> {
    None
}

impl TypesenseClient {
    async fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T, reqwest::Error> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-TYPESENSE-API-KEY",
            HeaderValue::from_str(&self.api_key).unwrap(),
        );

        reqwest::Client::new()
            .request(Method::GET, format!("http://localhost:8108/{}", endpoint))
            .headers(headers)
            .send()
            .await?
            .json::<T>()
            .await
    }

    async fn post<T, R>(&self, body: T, endpoint: &str) -> Result<R, reqwest::Error>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let mut headers = HeaderMap::new();

        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert(
            "X-TYPESENSE-API-KEY",
            HeaderValue::from_str(&self.api_key).unwrap(),
        );

        reqwest::Client::new()
            .request(Method::POST, format!("http://localhost:8108/{}", endpoint))
            .headers(headers)
            .body(serde_json::to_string(&body).unwrap())
            .send()
            .await?
            .json::<R>()
            .await
    }

    async fn post_with_echo<T>(&self, body: T, endpoint: &str) -> Result<(), reqwest::Error>
    where
        T: Serialize + DeserializeOwned,
    {
        self.post::<T, T>(body, endpoint).await.map(|_| ())
    }

    async fn create_collection(&self, create: CreateCollection) -> Result<(), reqwest::Error> {
        self.post_with_echo(create, "collections").await
    }

    /// Returns whether the collection was created
    pub async fn create_collection_if_not_exists(&self) -> Result<bool, reqwest::Error> {
        let collections: Vec<Collection> = self.get("collections").await?;

        if collections.is_empty() {
            self.create_collection(serde_json::from_str(TYPESENSE_SCHEMA).unwrap())
                .await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn add_word(&self, word: WordDocument) -> Result<(), reqwest::Error> {
        self.post::<_, Value>(word, "collections/words/documents")
            .await
            .map(|_| ())
    }

    pub async fn import_words(&self, words: Vec<WordDocument>) -> Result<(), reqwest::Error> {
        assert!(words.len() <= IMPORT_DOCUMENTS_LIMIT as usize);
        let json_lines = words
            .into_iter()
            .map(|w| serde_json::to_string(&w).unwrap())
            .join("\n");

        let mut headers = HeaderMap::new();

        headers.insert(
            "X-TYPESENSE-API-KEY",
            HeaderValue::from_str(&self.api_key).unwrap(),
        );

        #[derive(Deserialize)]
        struct Success {
            success: bool,
        }

        let res = reqwest::Client::new()
            .request(
                Method::POST,
                "http://localhost:8108/collections/words/documents/import?action=create",
            )
            .headers(headers)
            .body(json_lines)
            .send()
            .await?;

        let text = res.text().await?;

        for line in text.split("\n") {
            let res: Success = serde_json::from_str(line).unwrap();
            assert!(res.success);
        }

        Ok(())
    }

    pub async fn reindex_database(&self, db: Pool<SqliteConnectionManager>) {
        const SELECT: &str = "SELECT word_id, english, xhosa, part_of_speech, is_plural, noun_class FROM words ORDER BY word_id LIMIT ?1 OFFSET ?2;";

        let this = self.clone();
        task::spawn_blocking(move || {
            let conn = db.get().unwrap();
            let mut stmt = conn.prepare(SELECT).unwrap();
            let mut offset = 0;

            loop {
                let records = stmt
                    .query_map(params![IMPORT_DOCUMENTS_LIMIT, offset], |row| {
                        Ok(WordDocument {
                            id: row.get::<&str, i64>("word_id")?.to_string(),
                            english: row.get("english")?,
                            xhosa: row.get("xhosa")?,
                            part_of_speech: row.get("part_of_speech")?,
                            is_plural: row.get("is_plural")?,
                            noun_class: row.get("noun_class")?,
                        })
                    })
                    .unwrap();

                let words: Vec<WordDocument> = records.map(Result::unwrap).collect();
                let len = words.len();

                if len > 0 {
                    futures::executor::block_on(this.import_words(words)).unwrap();
                }

                if len < 40 {
                    break;
                } else {
                    offset += 40;
                }
            }
        })
        .await
        .unwrap();
    }

    pub async fn search_word_short(
        &self,
        query: &str,
    ) -> Result<ShortWordSearchResults, reqwest::Error> {
        let query_encoded = utf8_percent_encode(query, NON_ALPHANUMERIC);
        let endpoint = format!(
            "collections/words/documents/search?query_by=english,xhosa&q={}",
            query_encoded
        );
        self.get(&endpoint).await
    }
}
