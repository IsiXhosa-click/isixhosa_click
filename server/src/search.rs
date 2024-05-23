use crate::spawn_blocking_child;
use anyhow::{Context, Result};
use isixhosa::noun::NounClass;
use isixhosa_common::database::{GetWithSentinelExt, WordOrSuggestionId};
use isixhosa_common::language::{NounClassExt, PartOfSpeech, Transitivity};
use isixhosa_common::types::WordHit;
use num_enum::TryFromPrimitive;
use ordered_float::OrderedFloat;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::cmp::{max, Ordering};
use std::collections::HashSet;
use std::convert::{TryFrom, TryInto};
use std::fmt::{Debug, Formatter};
use std::num::NonZeroU64;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, FuzzyTermQuery, Query, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, INDEXED, STORED,
};
use tantivy::tokenizer::TextAnalyzer;
use tantivy::tokenizer::{LowerCaser, SimpleTokenizer};
use tantivy::{doc, Searcher};
use tantivy::{Index, IndexReader, IndexWriter, TantivyDocument, Term};
use tracing::{debug_span, info, info_span, instrument, Span};
use xtra::prelude::*;

const TANTIVY_WRITER_HEAP: usize = 128 * 1024 * 1024;
const RESULTS: usize = 10;

pub struct TantivyClient {
    schema_info: SchemaInfo,
    tokenizer: TextAnalyzer,
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

        let lowercaser = TextAnalyzer::builder(SimpleTokenizer::default())
            .filter(LowerCaser)
            .build();
        index.tokenizers().register("lowercaser", lowercaser);

        let num_searchers = num_cpus::get();
        let reader = index.reader_builder().try_into()?;

        let (searchers, mailbox) = Mailbox::bounded(32);

        let writer = index.writer_with_num_threads(1, TANTIVY_WRITER_HEAP)?;
        let tokenizer = index.tokenizer_for_field(schema_info.english).unwrap();
        let writer = WriterActor::new(writer, schema_info.clone());
        let writer = xtra::spawn_tokio(writer, Mailbox::bounded(16));

        let client = TantivyClient {
            schema_info,
            tokenizer,
            writer,
            searchers: searchers.clone(),
        };
        let client = Arc::new(client);

        for _ in 0..num_searchers {
            let actor = SearcherActor::new(reader.clone(), client.clone());
            xtra::spawn_tokio(actor, (searchers.clone(), mailbox.clone()));
        }

        if reindex {
            info!("Reindexing database");
            let now = Instant::now();
            client.reindex_database(db).await;
            info!(
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
        let is_informal = builder.add_u64_field("is_informal", STORED);
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
            is_informal,
            transitivity,
            noun_class,
            suggesting_user,
            existing_id,
            suggestion_id,
        }
    }

    #[instrument(
        name = "Search for a word",
        fields(
            query = %query,
            include = ?include,
            exact = duplicate,
        )
        skip_all,
    )]
    pub async fn search(
        &self,
        query: String,
        include: IncludeResults,
        duplicate: bool,
    ) -> Result<Vec<WordHit>> {
        self.searchers
            .send(SearchRequest {
                query,
                include,
                duplicate,
            })
            .await
            .map_err(Into::into)
    }

    #[instrument(name = "Reindex the database", skip_all)]
    pub async fn reindex_database(&self, db: Pool<SqliteConnectionManager>) {
        const SELECT: &str = "
            SELECT
                word_id, english, xhosa, part_of_speech, is_plural, is_inchoative, is_informal, transitivity,
                followed_by, noun_class
            FROM words
            ORDER BY word_id;
        ";

        let span = info_span!("Fetch all existing words").or_current();
        let docs = tokio::task::spawn_blocking(move || {
            let _g = span.enter();
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
                    is_informal: row.get("is_informal")?,
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

    fn add_word(
        writer: &mut IndexWriter,
        schema_info: &SchemaInfo,
        doc: WordDocument,
    ) -> Result<()> {
        let stemmed = if doc.part_of_speech == Some(PartOfSpeech::Verb) {
            // Remove (i) from latent i verbs
            doc.xhosa.trim_start_matches("(i)").to_owned()
        } else if doc.part_of_speech == Some(PartOfSpeech::Noun) || doc.part_of_speech.is_none() {
            // We just treat it as a noun for now.
            // TODO(isizulu): better stemming
            isixhosa::noun::guess_noun_base(&doc.xhosa, doc.noun_class)
        } else {
            doc.xhosa.to_owned()
        };

        let mut tantivy_doc = tantivy::doc!(
            schema_info.english => doc.english,
            schema_info.xhosa => doc.xhosa,
            schema_info.xhosa_stemmed => stemmed,
            schema_info.part_of_speech => doc.part_of_speech.map(|x| x as u64).unwrap_or(255),
            schema_info.suggesting_user => doc.suggesting_user.map(NonZeroU64::get).unwrap_or(0),
            schema_info.is_plural => doc.is_plural as u64,
            schema_info.is_inchoative => doc.is_inchoative as u64,
            schema_info.is_informal => doc.is_informal as u64,
            schema_info.transitivity => doc.transitivity.map(|x| x as u64).unwrap_or(255),
            schema_info.noun_class => doc.noun_class.map(|x| x as u64).unwrap_or(255),
        );

        let (id_field, suggestion) = match doc.id {
            WordOrSuggestionId::Suggested { suggestion_id } => {
                (schema_info.suggestion_id, suggestion_id)
            }
            WordOrSuggestionId::ExistingWord { existing_id } => {
                (schema_info.existing_id, existing_id)
            }
        };

        tantivy_doc.add_u64(id_field, suggestion);
        writer.add_document(tantivy_doc)?;

        Ok(())
    }
}

impl Actor for WriterActor {
    type Stop = ();

    async fn stopped(self) {}
}

#[derive(Debug)]
pub struct DeleteWord(WordOrSuggestionId);

#[derive(Debug)]
pub struct EditWord(WordDocument);

#[derive(Debug)]
pub struct ReindexWords(Vec<WordDocument>);

#[derive(Debug)]
pub struct IndexWord(WordDocument);

impl Handler<ReindexWords> for WriterActor {
    type Return = ();

    async fn handle(&mut self, docs: ReindexWords, _ctx: &mut xtra::Context<Self>) {
        let writer = self.writer.clone();
        let schema_info = self.schema_info.clone();

        spawn_blocking_child(move || {
            let mut writer = writer.lock().unwrap();
            writer.delete_all_documents().unwrap();

            for doc in docs.0 {
                Self::add_word(&mut writer, &schema_info, doc).unwrap();
            }

            writer.commit().unwrap();
        })
        .await
        .unwrap()
    }
}

impl Handler<IndexWord> for WriterActor {
    type Return = ();

    #[instrument(
        name = "Add a word to tantivy",
        fields(
            id = ?doc.0.id,
            is_suggestion = doc.0.suggesting_user.is_some(),
            suggesting_user = doc.0.suggesting_user,
        )
        skip_all,
    )]
    async fn handle(&mut self, doc: IndexWord, _ctx: &mut xtra::Context<Self>) {
        let writer = self.writer.clone();
        let schema_info = self.schema_info.clone();

        spawn_blocking_child(move || {
            let mut writer = writer.lock().unwrap();
            Self::add_word(&mut writer, &schema_info, doc.0).unwrap();
            writer.commit().unwrap();
        })
        .await
        .unwrap()
    }
}

impl Handler<EditWord> for WriterActor {
    type Return = ();

    #[instrument(
        name = "Edit a word in tantivy",
        fields(
            id = ?edit.0.id,
            suggesting_user = edit.0.suggesting_user,
        )
        skip_all,
    )]
    async fn handle(&mut self, edit: EditWord, _ctx: &mut xtra::Context<Self>) {
        let writer = self.writer.clone();
        let schema_info = self.schema_info.clone();

        spawn_blocking_child(move || {
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
            Self::add_word(&mut writer, &schema_info, edit.0).unwrap();
            writer.commit().unwrap();
        })
        .await
        .unwrap()
    }
}

impl Handler<DeleteWord> for WriterActor {
    type Return = ();

    #[instrument(name = "Delete a word from tantivy", fields(id = ?delete.0), skip_all)]
    async fn handle(&mut self, delete: DeleteWord, _ctx: &mut xtra::Context<Self>) {
        let writer = self.writer.clone();
        let schema_info = self.schema_info.clone();

        spawn_blocking_child(move || {
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

impl Actor for SearcherActor {
    type Stop = ();

    async fn stopped(self) {}
}

pub struct SearchRequest {
    query: String,
    include: IncludeResults,
    duplicate: bool,
}

#[allow(clippy::enum_variant_names)]
#[derive(Copy, Clone, Debug)]
pub enum IncludeResults {
    AcceptedOnly,
    AcceptedAndSuggestionsFrom(NonZeroU64),
    AcceptedAndAllSuggestions,
}

impl SearcherActor {
    #[instrument(
        name = "Search for a query in tantivy",
        fields(
            level = search_level,
            results,
        )
        skip_all
    )]
    fn query_terms(
        searcher: &mut Searcher,
        client: &TantivyClient,
        tokenizer: &mut TextAnalyzer,
        search_level: u8,
        req: &SearchRequest,
        out: &mut HashSet<WordHit>,
    ) {
        let mut tokenized = tokenizer.token_stream(&req.query);
        let mut queries: Vec<Box<dyn Query + 'static>> = Vec::with_capacity(3);
        tokenized.process(&mut |token| {
            let distance = match token.text.len() {
                0..=2 => 0,
                3..=5 => 1,
                _ => 2,
            };

            let distance = std::cmp::min(distance, search_level);

            let english = Term::from_field_text(client.schema_info.english, &token.text);
            let xhosa = Term::from_field_text(client.schema_info.xhosa, &token.text);
            let xhosa_stemmed =
                Term::from_field_text(client.schema_info.xhosa_stemmed, &token.text);

            let query_english = FuzzyTermQuery::new_prefix(english, distance, true);
            let query_xhosa = FuzzyTermQuery::new_prefix(xhosa, distance, true);
            let query_xhosa_stemmed = FuzzyTermQuery::new_prefix(xhosa_stemmed, distance, true);

            let this_term: Vec<Box<dyn Query + 'static>> = vec![
                Box::new(query_english),
                Box::new(query_xhosa),
                Box::new(query_xhosa_stemmed),
            ];

            queries.push(Box::new(BooleanQuery::union(this_term)));
        });

        let terms = BooleanQuery::intersection(queries);

        let not_suggestion = || {
            let not_suggestion = Term::from_field_u64(client.schema_info.suggesting_user, 0);
            let not_suggestion = TermQuery::new(not_suggestion, IndexRecordOption::Basic);
            BooleanQuery::intersection(vec![Box::new(not_suggestion), Box::new(terms.clone())])
        };

        let query = match req.include {
            IncludeResults::AcceptedAndAllSuggestions => terms,
            IncludeResults::AcceptedAndSuggestionsFrom(user) => {
                let suggested_by =
                    Term::from_field_u64(client.schema_info.suggesting_user, user.get());
                let suggested_by = TermQuery::new(suggested_by, IndexRecordOption::Basic);
                let suggested_by = BooleanQuery::intersection(vec![
                    Box::new(suggested_by),
                    Box::new(terms.clone()),
                ]);
                BooleanQuery::union(vec![Box::new(not_suggestion()), Box::new(suggested_by)])
            }
            IncludeResults::AcceptedOnly => not_suggestion(),
        };

        let mut count = 0;

        let iter = searcher
            .search(&query, &TopDocs::with_limit(RESULTS * 5)) // TODO unsure
            .unwrap()
            .into_iter()
            .map(|(_, doc_address)| {
                searcher
                    .doc(doc_address)
                    .map_err(anyhow::Error::from)
                    .and_then(|doc| WordHit::try_deserialize(&client.schema_info, doc))
                    .unwrap()
            })
            .inspect(|_| count += 1);

        out.extend(iter);

        Span::current().record("results", count);
    }
}

impl Handler<SearchRequest> for SearcherActor {
    type Return = Vec<WordHit>;

    async fn handle(
        &mut self,
        mut req: SearchRequest,
        _ctx: &mut xtra::Context<Self>,
    ) -> Vec<WordHit> {
        #[derive(PartialEq, Eq)]
        struct WordHitWithScore {
            hit: WordHit,
            score: OrderedFloat<f64>,
        }

        impl WordHitWithScore {
            fn new(hit: WordHit, query: &str) -> WordHitWithScore {
                let sim =
                    |hit: &str| OrderedFloat(strsim::jaro_winkler(query, &hit.to_lowercase()));
                let xh_sim = sim(hit.xhosa.trim_start_matches("(i)"));
                let en_sim = sim(&hit.english);
                // Temporary fix for "become ___" ranking very low
                let en_inchoative_sim = sim(hit.english.trim_start_matches("become "));
                let sim_score = max(xh_sim, max(en_sim, en_inchoative_sim));
                // 1% penalty to any informal words to make them rank lower (they are usually less relevant)
                let informal_penalty = if hit.is_informal { 0.99 } else { 1.0 };

                WordHitWithScore {
                    score: sim_score * informal_penalty,
                    hit,
                }
            }
        }

        impl PartialOrd for WordHitWithScore {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for WordHitWithScore {
            fn cmp(&self, other: &Self) -> Ordering {
                self.score.cmp(&other.score).reverse().then_with(|| {
                    self.hit
                        .part_of_speech
                        .cmp(&other.hit.part_of_speech)
                        .then(self.hit.is_plural.cmp(&other.hit.is_plural))
                        .then(self.hit.id.cmp(&other.hit.id))
                })
            }
        }

        req.query = req.query.to_lowercase().replace(['(', ')'], "");
        req.query.truncate(64);

        let mut searcher = self.reader.searcher();
        let client = self.client.clone();
        let mut tokenizer = self.client.tokenizer.clone();
        let mut results = HashSet::with_capacity(10);

        spawn_blocking_child(move || {
            for level in 0..=2 {
                SearcherActor::query_terms(
                    &mut searcher,
                    &client,
                    &mut tokenizer,
                    level,
                    &req,
                    &mut results,
                );

                if results.len() >= RESULTS {
                    break;
                }
            }

            if req.duplicate {
                let _g = debug_span!("Filtering for exact matches only").entered();

                let exact = |hit: &WordHit| {
                    hit.english.to_lowercase() == req.query.to_lowercase()
                        || hit.xhosa.to_lowercase() == req.query.to_lowercase()
                };
                Ok::<_, anyhow::Error>(results.into_iter().filter(exact).collect())
            } else {
                let _g =
                    info_span!("Sorting and ordering results", results = results.len()).entered();

                let mut results: Vec<WordHitWithScore> =
                    info_span!("Calculating string similarity").in_scope(|| {
                        results
                            .into_iter()
                            .map(|hit| WordHitWithScore::new(hit, &req.query))
                            .collect()
                    });

                debug_span!("Sorting list based on score").in_scope(|| results.sort());

                Ok(results.into_iter().take(RESULTS).map(|s| s.hit).collect())
            }
        })
        .await
        .expect("Error executing search task")
        .unwrap() // TODO(error handling)
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
    is_informal: Field,
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
    pub part_of_speech: Option<PartOfSpeech>,
    pub is_plural: bool,
    pub is_inchoative: bool,
    pub transitivity: Option<Transitivity>,
    /// This is only `Some` for indexed suggestions.
    pub suggesting_user: Option<NonZeroU64>,
    pub noun_class: Option<NounClass>,
    pub is_informal: bool,
}

trait WordHitExt {
    fn try_deserialize(schema_info: &SchemaInfo, doc: TantivyDocument) -> Result<WordHit>;
}

impl WordHitExt for WordHit {
    fn try_deserialize(schema_info: &SchemaInfo, doc: TantivyDocument) -> Result<WordHit> {
        let is_suggestion = doc
            .get_first(schema_info.suggesting_user)
            .and_then(|v| v.as_u64())
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

        fn get_str(document: &TantivyDocument, field: Field, name: &str) -> anyhow::Result<String> {
            document
                .get_first(field)
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned)
                .with_context(|| {
                    format!(
                        "Invalid value for `{}` field in document {:#?}",
                        name, document
                    )
                })
        }

        fn get_bool(document: &TantivyDocument, field: Field, name: &str) -> anyhow::Result<bool> {
            document
                .get_first(field)
                .and_then(|v| v.as_u64())
                .map(|v| v == 1)
                .with_context(|| {
                    format!(
                        "Invalid value for `{}` field in document {:#?}",
                        name, document
                    )
                })
        }

        fn get_with_sentinel<T>(document: &TantivyDocument, field: Field) -> Option<T>
        where
            T: TryFromPrimitive,
            T::Primitive: TryFrom<u64>,
        {
            document
                .get_first(field)
                .and_then(|v| v.as_u64())
                .and_then(|ord| T::try_from_primitive(ord.try_into().ok()?).ok())
        }

        Ok(WordHit {
            id: doc
                .get_first(id_field)
                .and_then(|v| v.as_u64())
                .with_context(|| format!("Invalid value for id field in document {:#?}", doc))?,
            english: get_str(&doc, schema_info.english, "english")?,
            xhosa: get_str(&doc, schema_info.xhosa, "xhosa")?,
            part_of_speech: get_with_sentinel(&doc, schema_info.part_of_speech),
            is_plural: get_bool(&doc, schema_info.is_plural, "is_plural")?,
            is_inchoative: get_bool(&doc, schema_info.is_inchoative, "is_inchoative")?,
            is_informal: get_bool(&doc, schema_info.is_informal, "is_informal")?,
            transitivity: get_with_sentinel(&doc, schema_info.transitivity),
            is_suggestion,
            noun_class: get_with_sentinel(&doc, schema_info.noun_class)
                .map(|c: NounClass| c.to_prefixes()),
        })
    }
}

impl From<WordDocument> for WordHit {
    fn from(d: WordDocument) -> Self {
        WordHit {
            id: d.id.inner(),
            english: d.english,
            xhosa: d.xhosa,
            part_of_speech: d.part_of_speech,
            is_plural: d.is_plural,
            is_inchoative: d.is_inchoative,
            is_informal: d.is_informal,
            transitivity: d.transitivity,
            is_suggestion: d.suggesting_user.is_some(),
            noun_class: d.noun_class.map(|c| c.to_prefixes()),
        }
    }
}
