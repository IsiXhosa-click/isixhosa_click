use crate::database::WordOrSuggestionId;

use crate::language::{PartOfSpeech, Transitivity};
use crate::serialization::GetWithSentinelExt;
use crate::serialization::{SerOnlyDisplay, SerializePrimitive};
use anyhow::{Context, Result};
use isixhosa::noun::NounClass;
use num_enum::TryFromPrimitive;
use ordered_float::OrderedFloat;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::Serialize;
use std::cmp::{max, Reverse};
use std::convert::{TryFrom, TryInto};

use std::fmt::{Debug, Formatter};
use std::num::NonZeroU64;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::doc;
use tantivy::query::{BooleanQuery, FuzzyTermQuery, Query, TermQuery};
use tantivy::schema::{
    Field, FieldValue, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, INDEXED,
    STORED,
};
use tantivy::tokenizer::TextAnalyzer;
use tantivy::tokenizer::{LowerCaser, SimpleTokenizer};
use tantivy::{Document, Index, IndexReader, IndexWriter, Term};
use xtra::spawn::TokioGlobalSpawnExt;
use xtra::{Actor, Address, Handler, Message};

const TANTIVY_WRITER_HEAP: usize = 128 * 1024 * 1024;

pub struct TantivyClient {
    schema_info: SchemaInfo,
    english_tokenizer: TextAnalyzer,
    writer: Address<WriterActor>,
    searchers: Address<SearcherActor>,
}

impl Debug for TantivyClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "TantivyClient {{ .. }}")
    }
}

impl TantivyClient {
    pub async fn start(
        path: &Path,
        db: Pool<SqliteConnectionManager>,
    ) -> Result<Arc<TantivyClient>> {
        let schema_info = Self::build_schema();
        let dir = MmapDirectory::open(path)
            .with_context(|| format!("Failed to open tantivy directory {:?}", path))?;
        let reindex = !Index::exists(&dir)?;
        let index = Index::open_or_create(dir, schema_info.schema.clone())?;

        let lowercaser = TextAnalyzer::from(SimpleTokenizer).filter(LowerCaser);
        index.tokenizers().register("lowercaser", lowercaser);

        let num_searchers = num_cpus::get();
        let reader = index
            .reader_builder()
            .num_searchers(num_searchers)
            .try_into()?;

        let (searchers, mut ctx) = xtra::Context::new(Some(32));

        let writer = index.writer(TANTIVY_WRITER_HEAP)?;

        let client = TantivyClient {
            schema_info: schema_info.clone(),
            english_tokenizer: index.tokenizer_for_field(schema_info.english).unwrap(),
            writer: WriterActor::new(writer, schema_info)
                .create(Some(16))
                .spawn_global(),
            searchers,
        };
        let client = Arc::new(client);

        for _ in 0..num_searchers {
            tokio::spawn(ctx.attach(SearcherActor::new(reader.clone(), client.clone())));
        }

        if reindex {
            log::info!("Reindexing database");
            let now = Instant::now();
            client.reindex_database(db).await;
            log::info!(
                "Database reindexed in {:.2}ms",
                now.elapsed().as_secs_f64() * 1_000.0
            );
        }

        Ok(client)
    }

    fn build_schema() -> SchemaInfo {
        let mut builder = Schema::builder();

        let text_options = TextOptions::default()
            .set_indexing_options(TextFieldIndexing::default().set_tokenizer("lowercaser"))
            .set_stored();

        let english = builder.add_text_field("english", text_options.clone());
        let xhosa = builder.add_text_field("xhosa", text_options.clone());
        let xhosa_stemmed = builder.add_text_field("xhosa_stemmed", text_options);
        let part_of_speech = builder.add_u64_field("part_of_speech", STORED);
        let is_plural = builder.add_u64_field("is_plural", STORED);
        let is_inchoative = builder.add_u64_field("is_inchoative", STORED);
        let transitivity = builder.add_u64_field("is_transitive", STORED);
        let noun_class = builder.add_u64_field("noun_class", STORED);
        let suggesting_user = builder.add_u64_field("is_suggestion", STORED | INDEXED);
        let existing_id = builder.add_u64_field("existing_id", STORED | INDEXED);
        let suggestion_id = builder.add_u64_field("suggestion_id", STORED | INDEXED);

        SchemaInfo {
            schema: builder.build(),
            english,
            xhosa,
            xhosa_stemmed,
            part_of_speech,
            is_plural,
            is_inchoative,
            transitivity,
            noun_class,
            suggesting_user,
            existing_id,
            suggestion_id,
        }
    }

    pub async fn search(
        &self,
        query: String,
        include_suggestions_from_user: Option<NonZeroU64>,
    ) -> Result<Vec<WordHit>> {
        self.searchers
            .send(SearchRequest {
                query,
                include_suggestions_from_user,
            })
            .await
            .map_err(Into::into)
    }

    pub async fn reindex_database(&self, db: Pool<SqliteConnectionManager>) {
        const SELECT: &str = "
            SELECT
                word_id, english, xhosa, part_of_speech, is_plural, is_inchoative, transitivity,
                followed_by, noun_class
            FROM words
            ORDER BY word_id;
        ";

        let docs = tokio::task::spawn_blocking(move || {
            let conn = db.get().unwrap();
            let mut stmt = conn.prepare(SELECT).unwrap();

            stmt.query_map(params![], |row| {
                Ok(WordDocument {
                    id: WordOrSuggestionId::existing(row.get::<&str, i64>("word_id")? as u64),
                    english: row.get("english")?,
                    xhosa: row.get("xhosa")?,
                    part_of_speech: row.get("part_of_speech")?,
                    is_plural: row.get("is_plural")?,
                    is_inchoative: row.get("is_inchoative")?,
                    transitivity: row.get_with_sentinel("transitivity")?,
                    suggesting_user: None,
                    noun_class: row.get_with_sentinel("noun_class")?,
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
        self.writer.send(EditWord(word)).await.unwrap()
    }

    pub async fn delete_word(&self, id: WordOrSuggestionId) {
        self.writer.send(DeleteWord(id)).await.unwrap()
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
        let stemmed = if doc.part_of_speech == PartOfSpeech::Noun {
            isixhosa::noun::guess_noun_base(&doc.xhosa, doc.noun_class)
        } else {
            doc.xhosa.clone()
        };

        let mut tantivy_doc = tantivy::doc!(
            schema_info.english => doc.english,
            schema_info.xhosa => doc.xhosa,
            schema_info.xhosa_stemmed => stemmed,
            schema_info.part_of_speech => doc.part_of_speech as u64,
            schema_info.suggesting_user => doc.suggesting_user.map(NonZeroU64::get).unwrap_or(0),
            schema_info.is_plural => doc.is_plural as u64,
            schema_info.is_inchoative => doc.is_inchoative as u64,
            schema_info.transitivity => doc.transitivity.map(|x| x as u64).unwrap_or(255),
            schema_info.noun_class => doc.noun_class.map(|x| x as u64).unwrap_or(255),
        );

        let id = match doc.id {
            WordOrSuggestionId::Suggested { suggestion_id } => {
                FieldValue::new(schema_info.suggestion_id, Value::U64(suggestion_id))
            }
            WordOrSuggestionId::ExistingWord { existing_id } => {
                FieldValue::new(schema_info.existing_id, Value::U64(existing_id))
            }
        };

        tantivy_doc.add(id);
        writer.add_document(tantivy_doc);
    }
}

impl Actor for WriterActor {}

#[derive(Debug)]
pub struct DeleteWord(WordOrSuggestionId);

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
        })
        .await
        .unwrap()
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
        })
        .await
        .unwrap()
    }
}

#[async_trait::async_trait]
impl Handler<EditWord> for WriterActor {
    async fn handle(&mut self, edit: EditWord, _ctx: &mut xtra::Context<Self>) {
        let writer = self.writer.clone();
        let schema_info = self.schema_info.clone();

        tokio::task::spawn_blocking(move || {
            let mut writer = writer.lock().unwrap();
            let term = match edit.0.id {
                WordOrSuggestionId::ExistingWord { existing_id } => {
                    Term::from_field_u64(schema_info.existing_id, existing_id)
                }
                WordOrSuggestionId::Suggested { suggestion_id } => {
                    Term::from_field_u64(schema_info.suggestion_id, suggestion_id)
                }
            };
            writer.delete_term(term);
            Self::add_word(&mut writer, &schema_info, edit.0);
            writer.commit().unwrap();
        })
        .await
        .unwrap()
    }
}

#[async_trait::async_trait]
impl Handler<DeleteWord> for WriterActor {
    async fn handle(&mut self, delete: DeleteWord, _ctx: &mut xtra::Context<Self>) {
        let writer = self.writer.clone();
        let schema_info = self.schema_info.clone();

        tokio::task::spawn_blocking(move || {
            let mut writer = writer.lock().unwrap();
            let term = match delete.0 {
                WordOrSuggestionId::ExistingWord { existing_id } => {
                    Term::from_field_u64(schema_info.existing_id, existing_id)
                }
                WordOrSuggestionId::Suggested { suggestion_id } => {
                    Term::from_field_u64(schema_info.suggestion_id, suggestion_id)
                }
            };
            writer.delete_term(term);
            writer.commit().unwrap();
        })
        .await
        .unwrap()
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

pub struct SearchRequest {
    query: String,
    include_suggestions_from_user: Option<NonZeroU64>,
}

impl Message for SearchRequest {
    type Result = Vec<WordHit>;
}

#[async_trait::async_trait]
impl Handler<SearchRequest> for SearcherActor {
    async fn handle(
        &mut self,
        mut req: SearchRequest,
        _ctx: &mut xtra::Context<Self>,
    ) -> Vec<WordHit> {
        req.query = req.query.to_lowercase().replace("(", "").replace(")", "");
        req.query.truncate(32);

        let searcher = self.reader.searcher();
        let client = self.client.clone();

        // Drop BoxTokenStream which is not Send
        let query = {
            let mut tokenized = client.english_tokenizer.token_stream(&req.query);
            let mut queries: Vec<Box<dyn Query + 'static>> = Vec::with_capacity(3);
            tokenized.process(&mut |token| {
                let distance = match token.text.len() {
                    0..=2 => 0,
                    3..=5 => 1,
                    _ => 2,
                };

                let english = Term::from_field_text(client.schema_info.english, &token.text);
                let xhosa = Term::from_field_text(client.schema_info.xhosa, &token.text);
                let xhosa_stemmed =
                    Term::from_field_text(client.schema_info.xhosa_stemmed, &token.text);

                let query_english = FuzzyTermQuery::new_prefix(english, distance, true);
                let query_xhosa = FuzzyTermQuery::new_prefix(xhosa, distance, true);
                let query_xhosa_stemmed = FuzzyTermQuery::new_prefix(xhosa_stemmed, distance, true);

                queries.reserve(3);
                queries.push(Box::new(query_english));
                queries.push(Box::new(query_xhosa));
                queries.push(Box::new(query_xhosa_stemmed));
            });

            let terms = BooleanQuery::union(queries);
            let not_suggestion = Term::from_field_u64(client.schema_info.suggesting_user, 0);
            let not_suggestion = TermQuery::new(not_suggestion, IndexRecordOption::Basic);
            let not_suggestion =
                BooleanQuery::intersection(vec![Box::new(not_suggestion), Box::new(terms.clone())]);

            match req.include_suggestions_from_user {
                Some(user) => {
                    let suggested_by =
                        Term::from_field_u64(client.schema_info.suggesting_user, user.get());
                    let suggested_by = TermQuery::new(suggested_by, IndexRecordOption::Basic);
                    let suggested_by =
                        BooleanQuery::intersection(vec![Box::new(suggested_by), Box::new(terms)]);
                    BooleanQuery::union(vec![Box::new(not_suggestion), Box::new(suggested_by)])
                }
                None => not_suggestion,
            }
        };

        // TODO(logging): find out when there are more than 64 top docs for a long-ish query to see
        // when it needs to be increased
        let top_docs = TopDocs::with_limit(64);

        tokio::task::spawn_blocking(move || {
            let mut results: Vec<WordHit> = searcher
                .search(&query, &top_docs)?
                .into_iter()
                .map(|(_score, doc_address)| {
                    searcher
                        .doc(doc_address)
                        .map_err(anyhow::Error::from)
                        .and_then(|doc| WordHit::try_deserialize(&client.schema_info, doc))
                })
                .collect::<Result<_>>()?;

            results.sort_by_cached_key(|hit| {
                Reverse(max(
                    OrderedFloat(strsim::jaro_winkler(&req.query, &hit.xhosa)),
                    OrderedFloat(strsim::jaro_winkler(&req.query, &hit.english)),
                ))
            });
            results.truncate(5);
            Ok::<_, anyhow::Error>(results)
        })
        .await
        .expect("Error executing search task")
        .unwrap() // TODO error handling
    }
}

#[derive(Clone, Debug)]
struct SchemaInfo {
    schema: Schema,
    english: Field,
    xhosa: Field,
    xhosa_stemmed: Field,
    part_of_speech: Field,
    is_plural: Field,
    is_inchoative: Field,
    transitivity: Field,
    noun_class: Field,
    suggesting_user: Field,
    existing_id: Field,
    suggestion_id: Field,
}

#[derive(Clone, Debug)]
pub struct WordDocument {
    pub id: WordOrSuggestionId,
    pub english: String,
    pub xhosa: String,
    pub part_of_speech: PartOfSpeech,
    pub is_plural: bool,
    pub is_inchoative: bool,
    pub transitivity: Option<Transitivity>,
    /// This is only `Some` for indexed suggestions.
    pub suggesting_user: Option<NonZeroU64>,
    pub noun_class: Option<NounClass>,
}

#[derive(Clone, Debug, Serialize, Hash, Eq, PartialEq)]
pub struct WordHit {
    pub id: u64,
    pub english: String,
    pub xhosa: String,
    pub part_of_speech: SerOnlyDisplay<PartOfSpeech>,
    pub is_plural: bool,
    pub is_inchoative: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transitivity: Option<SerOnlyDisplay<Transitivity>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub noun_class: Option<SerializePrimitive<NounClass, u8>>,
    pub is_suggestion: bool,
}

impl WordHit {
    pub fn empty() -> WordHit {
        WordHit {
            id: 0,
            english: String::new(),
            xhosa: String::new(),
            part_of_speech: SerOnlyDisplay(PartOfSpeech::Interjection),
            is_plural: false,
            is_inchoative: false,
            transitivity: None,
            noun_class: None,
            is_suggestion: false,
        }
    }

    fn try_deserialize(schema_info: &SchemaInfo, doc: Document) -> Result<WordHit> {
        let pos_ord = doc
            .get_first(schema_info.part_of_speech)
            .and_then(Value::u64_value)
            .with_context(|| {
                format!(
                    "Invalid value for field `part_of_speech` in document {:#?}",
                    doc
                )
            })?;
        let part_of_speech = SerOnlyDisplay(PartOfSpeech::try_from_primitive(pos_ord.try_into()?)?);

        let is_suggestion = doc
            .get_first(schema_info.suggesting_user)
            .and_then(Value::u64_value)
            .map(|v| v != 0)
            .with_context(|| {
                format!(
                    "Invalid value for field `suggesting_user` in document {:#?}",
                    doc
                )
            })?;

        let id_field = if is_suggestion {
            schema_info.suggestion_id
        } else {
            schema_info.existing_id
        };

        fn get_str(document: &Document, field: Field, name: &str) -> anyhow::Result<String> {
            document
                .get_first(field)
                .and_then(Value::text)
                .map(ToOwned::to_owned)
                .with_context(|| {
                    format!(
                        "Invalid value for `{}` field in document {:#?}",
                        name, document
                    )
                })
        }

        fn get_bool(document: &Document, field: Field, name: &str) -> anyhow::Result<bool> {
            document
                .get_first(field)
                .and_then(Value::u64_value)
                .map(|v| v == 1)
                .with_context(|| {
                    format!(
                        "Invalid value for `{}` field in document {:#?}",
                        name, document
                    )
                })
        }

        fn get_with_sentinel<T>(document: &Document, field: Field) -> Option<T>
        where
            T: TryFromPrimitive,
            T::Primitive: TryFrom<u64>,
        {
            document
                .get_first(field)
                .and_then(Value::u64_value)
                .and_then(|ord| T::try_from_primitive(ord.try_into().ok()?).ok())
        }

        Ok(WordHit {
            id: doc
                .get_first(id_field)
                .and_then(Value::u64_value)
                .with_context(|| format!("Invalid value for id field in document {:#?}", doc))?,
            english: get_str(&doc, schema_info.english, "english")?,
            xhosa: get_str(&doc, schema_info.xhosa, "xhosa")?,
            part_of_speech,
            is_plural: get_bool(&doc, schema_info.is_plural, "is_plural")?,
            is_inchoative: get_bool(&doc, schema_info.is_inchoative, "is_inchoative")?,
            transitivity: get_with_sentinel(&doc, schema_info.transitivity).map(SerOnlyDisplay),
            is_suggestion,
            noun_class: get_with_sentinel(&doc, schema_info.noun_class)
                .map(SerializePrimitive::new),
        })
    }
}

impl From<WordDocument> for WordHit {
    fn from(d: WordDocument) -> Self {
        WordHit {
            id: d.id.inner(),
            english: d.english,
            xhosa: d.xhosa,
            part_of_speech: SerOnlyDisplay(d.part_of_speech),
            is_plural: d.is_plural,
            is_inchoative: d.is_inchoative,
            transitivity: d.transitivity.map(SerOnlyDisplay),
            is_suggestion: d.suggesting_user.is_some(),
            noun_class: d.noun_class.map(SerializePrimitive::new),
        }
    }
}
