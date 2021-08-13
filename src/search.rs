use serde::{Serialize, Deserialize};
use tantivy::schema::{Schema, STORED, STRING, Field, Value, INDEXED};
use crate::language::{SerializeDisplay, PartOfSpeech, NounClass, NounClassOpt, NounClassOptExt};
use std::sync::{Arc, Mutex};
use tantivy::{Index, IndexReader, IndexWriter, Document, Term};
use tantivy::directory::MmapDirectory;
use anyhow::{Context, Result};
use xtra::{Address, Actor, Message, Handler};
use tantivy::query::FuzzyTermQuery;
use tantivy::collector::TopDocs;
use std::convert::TryInto;
use num_enum::TryFromPrimitive;
use r2d2::Pool;
use tantivy::doc;
use rusqlite::params;
use r2d2_sqlite::SqliteConnectionManager;
use xtra::spawn::TokioGlobalSpawnExt;
use ordered_float::OrderedFloat;
use std::path::Path;

const TANTIVY_WRITER_HEAP: usize = 128 * 1024 * 1024;

pub struct TantivyClient {
    schema_info: SchemaInfo,
    writer: Address<WriterActor>,
    searchers: Address<SearcherActor>,
}

impl TantivyClient {
    pub async fn start(path: &Path, db: Pool<SqliteConnectionManager>) -> Result<Arc<TantivyClient>> {
        let schema_info = Self::build_schema();
        let dir = MmapDirectory::open(path)
            .with_context(|| format!("Failed to open tantivy directory {:?}", path))?;
        let reindex = !Index::exists(&dir)?;
        let index = Index::open_or_create(dir, schema_info.schema.clone())?;

        let num_searchers = num_cpus::get(); // TODO config
        let reader = index.reader_builder()
            .num_searchers(num_searchers)
            .try_into()?;

        let (searchers, mut ctx) = xtra::Context::new(Some(32));

        let writer = index.writer(TANTIVY_WRITER_HEAP)?;

        let client = TantivyClient {
            schema_info: schema_info.clone(),
            writer: WriterActor::new(writer, schema_info).create(Some(16)).spawn_global(),
            searchers,
        };
        let client = Arc::new(client);

        for _ in 0..num_searchers {
            tokio::spawn(ctx.attach(SearcherActor::new(reader.clone(), client.clone())));
        }

        if reindex {
            client.reindex_database(db).await;
            eprintln!("Database reindexed");
        }

        Ok(client)
    }

    fn build_schema() -> SchemaInfo {
        let mut builder = Schema::builder();

        let english = builder.add_text_field("english", STRING | STORED);
        let xhosa = builder.add_text_field("xhosa", STRING | STORED);
        let part_of_speech = builder.add_u64_field("part_of_speech",STORED);
        let is_plural = builder.add_u64_field("is_plural", STORED);
        let noun_class = builder.add_u64_field("noun_class", STORED);
        let id = builder.add_u64_field("id", STORED | INDEXED);

        SchemaInfo {
            schema: builder.build(),
            english,
            xhosa,
            part_of_speech,
            is_plural,
            noun_class,
            id,
        }
    }

    pub async fn search(&self, query: String) -> Result<Vec<WordHit>> {
        self.searchers.send(SearchRequest(query)).await.map_err(Into::into)
    }

    pub async fn reindex_database(&self, db: Pool<SqliteConnectionManager>) {
        const SELECT: &str = "SELECT word_id, english, xhosa, part_of_speech, is_plural, noun_class FROM words ORDER BY word_id;";

        let docs = tokio::task::spawn_blocking(move || {
            let conn = db.get().unwrap();
            let mut stmt = conn.prepare(SELECT).unwrap();

            stmt
                .query_map(params![], |row| {
                    Ok(WordDocument {
                        id: row.get::<&str, i64>("word_id")? as u64,
                        english: row.get("english")?,
                        xhosa: row.get("xhosa")?,
                        part_of_speech: row.get("part_of_speech")?,
                        is_plural: row.get("is_plural")?,
                        noun_class: row.get::<&str, Option<NounClassOpt>>("noun_class")?.flatten(),
                    })
                })
                .unwrap()
                .collect::<Result<Vec<WordDocument>, _>>()
                .unwrap()
        })
        .await
        .unwrap();

        self.writer.send(ReindexWords(docs)).await.unwrap();
    }

    pub async fn add_new_word(&self, word: WordDocument) {
        self.writer.send(IndexWord(word)).await.unwrap()
    }

    pub async fn edit_word(&self, word: WordDocument) {
        eprintln!("Edit word!");
        self.writer.send(EditWord(word)).await.unwrap()
    }

    pub async fn delete_word(&self, word_id: u64) {
        self.writer.send(DeleteWord(word_id)).await.unwrap()
    }
}

pub struct WriterActor {
    writer: Arc<Mutex<IndexWriter>>,
    schema_info: Arc<SchemaInfo>,
}

impl WriterActor {
    fn new(writer: IndexWriter, schema_info: SchemaInfo) -> Self {
        WriterActor {
            writer: Arc::new(Mutex::new(writer)),
            schema_info: Arc::new(schema_info),
        }
    }

    fn add_word(writer: &mut IndexWriter, schema_info: &SchemaInfo, doc: WordDocument) {
        writer.add_document(tantivy::doc!(
            schema_info.id => doc.id,
            schema_info.english => doc.english,
            schema_info.xhosa => doc.xhosa,
            schema_info.part_of_speech => doc.part_of_speech as u64,
            schema_info.is_plural => doc.is_plural as u64,
            schema_info.noun_class => doc.noun_class.map(|x| x as u64).unwrap_or(255),
        ));
    }
}

// TODO perhaps commit more infrequently?
impl Actor for WriterActor {}

#[derive(Debug)]
pub struct DeleteWord(u64);

impl Message for DeleteWord {
    type Result = ();
}

#[derive(Debug)]
pub struct EditWord(WordDocument);

impl Message for EditWord {
    type Result = ();
}

#[derive(Debug)]
pub struct ReindexWords(Vec<WordDocument>);

impl Message for ReindexWords {
    type Result = ();
}

#[derive(Debug)]
pub struct IndexWord(WordDocument);

impl Message for IndexWord {
    type Result = ();
}

#[async_trait::async_trait]
impl Handler<ReindexWords> for WriterActor {
    async fn handle(&mut self, docs: ReindexWords, _ctx: &mut xtra::Context<Self>) {
        let writer = self.writer.clone();
        let schema_info = self.schema_info.clone();

        tokio::task::spawn_blocking(move || {
            let mut writer = writer.lock().unwrap();
            writer.delete_all_documents().unwrap();

            for doc in docs.0 {
                Self::add_word(&mut writer, &schema_info, doc);
            }

            writer.commit().unwrap();
        }).await.unwrap()
    }
}

#[async_trait::async_trait]
impl Handler<IndexWord> for WriterActor {
    async fn handle(&mut self, doc: IndexWord, _ctx: &mut xtra::Context<Self>) {
        let writer = self.writer.clone();
        let schema_info = self.schema_info.clone();

        tokio::task::spawn_blocking(move || {
            let mut writer = writer.lock().unwrap();
            Self::add_word(&mut writer, &schema_info, doc.0);
            writer.commit().unwrap();
        }).await.unwrap()
    }
}


#[async_trait::async_trait]
impl Handler<EditWord> for WriterActor {
    async fn handle(&mut self, edit: EditWord, _ctx: &mut xtra::Context<Self>) {
        let writer = self.writer.clone();
        let schema_info = self.schema_info.clone();

        tokio::task::spawn_blocking(move || {
            let mut writer = writer.lock().unwrap();
            let term = Term::from_field_u64(schema_info.id, edit.0.id);
            writer.delete_term(term);
            writer.commit().unwrap();

            Self::add_word(&mut writer, &schema_info, edit.0);
            writer.commit().unwrap();
        }).await.unwrap()
    }
}


#[async_trait::async_trait]
impl Handler<DeleteWord> for WriterActor {
    async fn handle(&mut self, delete: DeleteWord, _ctx: &mut xtra::Context<Self>) {
        let writer = self.writer.clone();
        let schema_info = self.schema_info.clone();

        tokio::task::spawn_blocking(move || {
            let mut writer = writer.lock().unwrap();
            let term = Term::from_field_u64(schema_info.id, delete.0);
            writer.delete_term(term);
            writer.commit().unwrap();
        }).await.unwrap()
    }
}

pub struct SearcherActor {
    reader: IndexReader,
    client: Arc<TantivyClient>,
}

impl SearcherActor {
    fn new(reader: IndexReader, client: Arc<TantivyClient>) -> Self {
        SearcherActor { reader, client }
    }
}

impl Actor for SearcherActor {}

pub struct SearchRequest(pub String);

impl Message for SearchRequest {
    type Result = Vec<WordHit>;
}

#[async_trait::async_trait]
impl Handler<SearchRequest> for SearcherActor {
    async fn handle(&mut self, req: SearchRequest, _ctx: &mut xtra::Context<Self>) -> Vec<WordHit> {
        let searcher = self.reader.searcher();
        let client = self.client.clone();
        let english = Term::from_field_text(client.schema_info.english, &req.0);
        let xhosa = Term::from_field_text(client.schema_info.xhosa, &req.0);

        let distance = 2;
        let max_results = 5;

        let query_english = FuzzyTermQuery::new_prefix(english, distance, true);
        let query_xhosa = FuzzyTermQuery::new_prefix(xhosa, distance, true);
        let top_docs = TopDocs::with_limit(max_results);

        tokio::task::spawn_blocking(move || {
            let mut results = searcher.search(&query_english, &top_docs).unwrap();
            let mut results2 = searcher.search(&query_xhosa, &top_docs)?;
            results.append(&mut results2);
            results.dedup_by_key(|(_score, doc_address)| *doc_address);
            results.sort_by_key(|(score, _doc_address)| OrderedFloat(*score));

            results.into_iter()
                .take(max_results)
                .map(|(_score, doc_address)| searcher
                    .doc(doc_address)
                    .map_err(anyhow::Error::from)
                    .and_then(|doc| WordHit::try_deserialize(&client.schema_info, doc))
                )
                .collect::<Result<Vec<_>, _>>()
        }).await.expect("Error executing search task").unwrap() // TODO error handling
    }
}

#[derive(Clone, Debug)]
struct SchemaInfo {
    schema: Schema,
    english: Field,
    xhosa: Field,
    part_of_speech: Field,
    is_plural: Field,
    noun_class: Field,
    id: Field,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WordDocument {
    pub id: u64,
    pub english: String,
    pub xhosa: String,
    pub part_of_speech: PartOfSpeech,
    pub is_plural: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default = "option_none")]
    pub noun_class: Option<NounClass>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WordHit {
    pub id: u64,
    pub english: String,
    pub xhosa: String,
    pub part_of_speech: SerializeDisplay<PartOfSpeech>,
    pub is_plural: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default = "option_none")]
    pub noun_class: Option<NounClass>,
}

impl WordHit {
    pub fn empty() -> WordHit {
        WordHit {
            id: 0,
            english: String::new(),
            xhosa: String::new(),
            part_of_speech: SerializeDisplay(PartOfSpeech::Other),
            is_plural: false,
            noun_class: None,
        }
    }

    // TODO better way?
    fn try_deserialize(schema_info: &SchemaInfo, document: Document) -> Result<WordHit> {
        let pos_ord = document
            .get_first(schema_info.part_of_speech)
            .and_then(Value::u64_value)
            .with_context(|| format!("Invalid value for field `part_of_speech` in document {:#?}", document))?;
        let part_of_speech = SerializeDisplay(PartOfSpeech::try_from_primitive(pos_ord.try_into()?)?);

        Ok(WordHit {
            id: document.get_first(schema_info.id)
                .and_then(Value::u64_value)
                .with_context(|| format!("Invalid value for field `id` in document {:#?}", document))?,
            english: document
                .get_first(schema_info.english)
                .and_then(Value::text)
                .with_context(|| format!("Invalid value for field `english` in document {:#?}", document))?
                .to_owned(),
            xhosa: document
                .get_first(schema_info.xhosa)
                .and_then(Value::text)
                .with_context(|| format!("Invalid value for field `xhosa` in document {:#?}", document))?
                .to_owned(),
            part_of_speech,
            is_plural: document
                .get_first(schema_info.is_plural)
                .and_then(Value::u64_value)
                .map(|v| v == 1)
                .with_context(|| format!("Invalid value for field `is_plural` in document {:#?}", document))?,
            noun_class: document
                .get_first(schema_info.noun_class)
                .and_then(Value::u64_value)
                .and_then(|ord| NounClass::try_from_primitive(ord.try_into().unwrap_or(255)).ok()),
        })
    }
}

impl From<WordDocument> for WordHit {
    fn from(d: WordDocument) -> Self {
        WordHit {
            id: d.id,
            english: d.english,
            xhosa: d.xhosa,
            part_of_speech: SerializeDisplay(d.part_of_speech),
            is_plural: d.is_plural,
            noun_class: d.noun_class,
        }
    }
}

fn option_none<T>() -> Option<T> {
    None
}