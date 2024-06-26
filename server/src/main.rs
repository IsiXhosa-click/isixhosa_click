// Soon after launch, perhaps before:
// - informal/archaic meanings
// - standalone example & linked word suggestion editing
// - forum for xhosa questions
// - error handling - dont crash always probably & on panic, always crash (viz. tokio workers)!
// - better search engine optimisation
// - cache control headers/etags
// - attributions - references
// - learn page with additional resources/links page

// Well after launch:
// - rate limiting
// - integration testing
// - tracing for logging over log: open telemetry/ELK stack or similar?
// - conjugation tables
// - user profiles showing statistics (for mods primarily but maybe can publicise it?)
// - semantic fields/categories linking related words to browse all at once
// - grammar notes
// - embedded blog (static site generator?) for transparency

use crate::auth::*;
use crate::database::suggestion::SuggestedWord;
use crate::search::{IncludeResults, TantivyClient};
use crate::serialization::false_fn;
use crate::session::LiveSearchSession;
use askama::Template;
use auth::auth;
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use details::details;
use edit::edit;
use futures::StreamExt;
use isixhosa_common::auth::{Auth, Permissions};
use isixhosa_common::database::{DbBase, ModeratorAccessDb, PublicAccessDb};
use isixhosa_common::format::DisplayHtml;
use isixhosa_common::types::{ExistingWord, WordHit};
use moderation::moderation;
use opentelemetry::{global, KeyValue};
use opentelemetry_sdk::Resource;
use percent_encoding::NON_ALPHANUMERIC;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::collections::HashSet;
use std::convert::Infallible;
use std::fmt::Debug;
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use submit::submit;
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, Span};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, Registry};
use warp::filters::compression::gzip;
#[cfg(debug_assertions)]
use warp::filters::BoxedFilter;
use warp::http::header::{CACHE_CONTROL, CONTENT_TYPE, LAST_MODIFIED};
use warp::http::{HeaderValue, StatusCode, Uri};
use warp::path::FullPath;
use warp::reject::{MethodNotAllowed, Reject};
use warp::reply::Response;
use warp::{path, reply, Filter, Rejection, Reply};
use warp_reverse_proxy as proxy;
use xtra::{Handler, Mailbox, WeakAddress};

mod auth;
mod config;
mod database;
mod details;
mod edit;
mod export;
mod import_zulu;
mod moderation;
mod search;
mod serialization;
mod session;
mod submit;
mod user_management;

pub use config::Config;

const STATIC_LAST_CHANGED: &str = env!("STATIC_LAST_CHANGED");
const STATIC_BIN_FILES_LAST_CHANGED: &str = env!("STATIC_BIN_FILES_LAST_CHANGED");

#[derive(Parser)]
#[command(name = "IsiXhosa.click")]
#[command(about = "Online, live dictionary software", long_about = None)]
struct CliArgs {
    /// The site. Each site has a distinct database, export directory, and config file.
    #[arg(short, long, required = true)]
    site: String,
    /// Whether to enable OpenTelemetry protocol (OTLP) trace exporting.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    with_otlp: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the server for the site
    Run,
    /// Run the backup for the site. The directory of the exported files is specified in the site's
    /// configuration file.
    Backup,
    /// Restore from the backup. The directory of the files to restore from are specified in the
    /// site's configuration file.
    Restore,
    /// Import a dictionary file in the format of the isiZulu LSP
    ImportZuluLSP {
        /// The path of the dictionary file
        path: PathBuf,
    },
    /// Commands relating to user management
    User(UserCommandArgs),
}

#[derive(Parser)]
struct UserCommandArgs {
    #[command(subcommand)]
    command: UserCommand,
}

#[derive(Subcommand, Clone)]
enum UserCommand {
    /// Set a user's permissions
    SetRole {
        /// The user's email
        user: String,
        /// The user's new role
        role: Permissions,
    },
    /// Lock a user so that they cannot log in - this amounts to a ban but is not necessarily
    /// because of bad behaviour (e.g., the user could have disabled their account voluntarily).
    Lock {
        /// The user's email
        user: String,
    },
    /// Unlock a user so they can log in again.
    Unlock {
        /// The user's email
        user: String,
    },
    /// List all users
    List,
    /// Logs out all users
    LogoutAll,
}

fn main() {
    let cli = CliArgs::parse();
    let cfg: Config = confy::load("isixhosa_click", Some(cli.site.as_ref())).unwrap();

    match cli.command {
        Commands::Run => {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(server(cfg, cli));
        }
        Commands::Backup => export::run_daily_tasks(cfg),
        Commands::Restore => export::restore(cfg),
        Commands::ImportZuluLSP { path } => import_zulu::import_zulu_lsp(cfg, &path).unwrap(),
        Commands::User(command) => user_management::run_command(cfg, command.command),
    }
}

pub fn spawn_blocking_child<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let span = Span::current();
    tokio::task::spawn_blocking(move || {
        let _g = span.enter();
        f()
    })
}

pub trait DebugBoxedExt: Filter {
    #[cfg(debug_assertions)]
    fn debug_boxed(self) -> BoxedFilter<Self::Extract>;

    #[cfg(not(debug_assertions))]
    fn debug_boxed(self) -> Self;
}

impl<F> DebugBoxedExt for F
where
    F: Filter + Send + Sync + 'static,
    F::Extract: Send,
    F::Error: Into<Rejection>,
{
    #[cfg(debug_assertions)]
    fn debug_boxed(self) -> BoxedFilter<Self::Extract> {
        self.boxed()
    }

    #[cfg(not(debug_assertions))]
    fn debug_boxed(self) -> Self {
        self
    }
}

pub trait DebugExt {
    fn to_debug(&self) -> String;
}

impl<T: Debug> DebugExt for T {
    fn to_debug(&self) -> String {
        format!("{:?}", self)
    }
}

#[derive(Debug)]
struct TemplateError(askama::Error);

impl Reject for TemplateError {}

fn init_tracing(cli: &CliArgs) {
    global::set_text_map_propagator(opentelemetry_jaeger_propagator::Propagator::new());
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .with_trace_config(
            opentelemetry_sdk::trace::config().with_resource(Resource::new(vec![KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                format!("isixhosa-click-{}", cli.site),
            )])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .unwrap();

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let fmt_layer = tracing_subscriber::fmt::layer().with_target(false);

    let registry = Registry::default().with(LevelFilter::DEBUG).with(fmt_layer);

    if cli.with_otlp {
        registry.with(telemetry).init();
    } else {
        registry.init();
    }
}

#[instrument(
    name = "Minify outgoing data",
    fields(unminified, minified, saving),
    skip_all
)]
async fn process_body<F, E>(response: Response, minify: F) -> Result<Response, Rejection>
where
    F: FnOnce(&str) -> Result<String, E>,
    E: Debug,
{
    let span = Span::current();
    let (parts, body) = response.into_parts();
    let bytes = warp::hyper::body::to_bytes(body).await.unwrap();
    let unminified = std::str::from_utf8(&bytes).unwrap();
    let minified = minify(unminified).unwrap();

    span.record("unminified", unminified.len());
    span.record("minified", minified.len());

    let saving = if !unminified.is_empty() {
        (1.0 - (minified.len() as f64 / unminified.len() as f64)) * 100.0
    } else {
        0.0
    };

    span.record("saving", format!("{:.2}%", saving));

    Ok(Response::from_parts(parts, minified.into()))
}

async fn minify_and_cache<R: Reply>(reply: R) -> Result<impl Reply, Rejection> {
    let response = reply.into_response();

    fn starts_with(mime: &str, pats: &[&str]) -> bool {
        pats.iter().any(|pat| mime.starts_with(pat))
    }

    if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
        let mime = &content_type.to_str().unwrap().to_owned();

        // TODO: we can't use minifier as it breaks the WASM bindgen wrapper:
        // https://github.com/GuillaumeGomez/minifier-rs/issues/108
        let mut response = if mime.starts_with("text/html") {
            #[allow(clippy::redundant_closure)] // lifetime issue
            process_body(response, |s| html_minifier::minify(s)).await?
        } else if mime.starts_with("text/css") {
            process_body(response, |s| {
                minifier::css::minify(s).map(|s| s.to_string())
            })
            .await?
        } else {
            response
        };

        if starts_with(mime, &["text", "application/javascript"]) && !mime.contains("charset=UTF-8")
        {
            let new_content_type =
                HeaderValue::from_str(&format!("{}; charset=UTF-8", mime)).unwrap();
            response
                .headers_mut()
                .insert(CONTENT_TYPE, new_content_type);
        }

        if mime.starts_with("font/woff2") {
            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000"),
            );
        }

        Ok(response)
    } else {
        Ok(response)
    }
}

#[instrument(name = "Handle errors")]
async fn handle_error(err: Rejection) -> Result<Response, Rejection> {
    if let Some(unauthorized) = err.find::<Unauthorized>() {
        let redirect_to = |to| {
            warp::http::Response::builder()
                .status(StatusCode::FOUND)
                .header(warp::http::header::LOCATION, to)
                .body("")
                .unwrap()
                .into_response()
        };

        match unauthorized.reason {
            UnauthorizedReason::NotLoggedIn => {
                debug!("User was not logged in; redirecting");
                let login = format!(
                    "/login/oauth2/authorization/oidc?redirect={}",
                    percent_encoding::utf8_percent_encode(
                        unauthorized.redirect.as_str(),
                        NON_ALPHANUMERIC
                    ),
                );

                Ok(redirect_to(login))
            }
            UnauthorizedReason::NoPermissions | UnauthorizedReason::Locked => {
                debug!("User has insufficient permissions");
                Ok(reply::with_status(warp::reply(), StatusCode::FORBIDDEN).into_response())
            }
            UnauthorizedReason::InvalidCookie => {
                debug!("User has invalid cookie; redirecting");
                Ok(redirect_to("/login/oauth2/authorization/oidc".to_owned()))
            }
        }
    } else if err.find::<MethodNotAllowed>().is_some() {
        Err(warp::reject::not_found())
    } else {
        Err(err)
    }
}

#[instrument("Set up database PRAGMAs and tables", skip_all)]
fn set_up_db(conn: &Connection) {
    const CREATIONS: [&str; 12] = [
        include_str!("sql/users.sql"),
        include_str!("sql/words.sql"),
        include_str!("sql/user_attributions.sql"),
        include_str!("sql/word_suggestions.sql"),
        include_str!("sql/word_deletion_suggestions.sql"),
        include_str!("sql/examples.sql"),
        include_str!("sql/example_suggestions.sql"),
        include_str!("sql/example_deletion_suggestions.sql"),
        include_str!("sql/linked_words.sql"),
        include_str!("sql/linked_word_suggestions.sql"),
        include_str!("sql/linked_word_deletion_suggestions.sql"),
        include_str!("sql/login_tokens.sql"),
    ];

    // See https://github.com/the-lean-crate/criner/discussions/5
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA wal_autocheckpoint = 1000;
        PRAGMA wal_checkpoint(TRUNCATE);
    ",
    )
    .unwrap();

    for creation in &CREATIONS {
        conn.execute(creation, params![]).unwrap();
    }
}

// I cannot be bothered trying to find the right type
macro_rules! wrap_filter {
    ($f:expr) => {
        $f
            .and_then(minify_and_cache)
            .with(warp::trace(|info| {
                tracing::info_span!(
                    "HTTPS request",
                    method = %info.method(),
                    path = %info.path(),
                )
            }))
            .with(warp::reply::with::header(warp::http::header::X_FRAME_OPTIONS, "Deny"))
            .with(gzip())
    }
}

async fn server(cfg: Config, args: CliArgs) {
    init_tracing(&args);
    info!("IsiXhosa server startup");

    let manager = SqliteConnectionManager::file(&cfg.database_path);
    let pool = Pool::new(manager).unwrap();
    let pool_clone = pool.clone();
    spawn_blocking_child(move || set_up_db(&pool_clone.get().unwrap()))
        .await
        .unwrap();

    let tantivy = TantivyClient::start(&cfg.tantivy_path, pool.clone())
        .await
        .unwrap();

    let tantivy_cloned = tantivy.clone();
    let with_tantivy = warp::any().map(move || tantivy_cloned.clone());
    let db = DbBase::new(pool);

    let search = {
        let search_page = with_any_auth(db.clone()).map(|auth, _db| Search {
            auth,
            hits: Default::default(),
            query: Default::default(),
        });

        let query_search = path::end()
            .and(warp::query())
            .and(with_tantivy.clone())
            .and(with_any_auth(db.clone()))
            .and_then(query_search);
        let live_search = path::end()
            .and(warp::ws())
            .and(with_tantivy.clone())
            .and(warp::query())
            .and(with_any_auth(db.clone()))
            .map(live_search);
        let duplicate_search = warp::path("duplicates")
            .and(path::end())
            .and(warp::query())
            .and(with_tantivy)
            .and(with_moderator_auth(db.clone()))
            .and_then(duplicate_search);

        warp::path("search")
            .and(
                duplicate_search
                    .or(live_search)
                    .or(query_search)
                    .or(search_page),
            )
            .debug_boxed()
    };

    let simple_templates = {
        let terms_of_use = warp::path("terms_of_use")
            .and(path::end())
            .and(with_any_auth(db.clone()))
            .map(|auth, _| TermsOfUse { auth });
        let style_guide = warp::path("style_guide")
            .and(path::end())
            .and(with_any_auth(db.clone()))
            .map(|auth, _| StyleGuide { auth });
        let wordle = warp::path("wordle")
            .and(path::end())
            .and(with_any_auth(db.clone()))
            .map(|auth, _| Wordle { auth });
        let offline = warp::get()
            .and(warp::path("offline"))
            .and(path::end())
            .map(|| Offline);

        let about = warp::get()
            .and(warp::path("about"))
            .and(path::end())
            .and(with_any_auth(db.clone()))
            .and_then(|auth, db| async move {
                Ok::<About, Infallible>(About {
                    auth,
                    word_count: spawn_blocking_child(move || ExistingWord::count_all(&db))
                        .await
                        .unwrap(),
                })
            });

        let walk = || {
            walkdir::WalkDir::new(&cfg.static_site_files)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
        };

        fn ends_with(entry: &str, pats: &[&str]) -> bool {
            pats.iter().any(|pat| entry.ends_with(pat))
        }

        let (bin_files, static_files) = walk()
            .map(|entry| {
                entry
                    .path()
                    .strip_prefix(&cfg.static_site_files)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned()
            })
            .filter(|entry: &String| !entry.contains("LICENSE"))
            .partition::<Vec<_>, _>(|entry| ends_with(entry, &["png", "svg", "woff2", "ico"]));

        let last_modified_static = walk()
            .filter_map(|entry| entry.metadata().ok())
            .filter_map(|meta| meta.modified().or(meta.created()).ok())
            .max()
            .unwrap();

        let last_modified_static = DateTime::<Utc>::from(last_modified_static);
        let last_modified_js = Utc::now(); // server boot time
        let last_modified = std::cmp::max(last_modified_static, last_modified_js);
        let last_modified = last_modified.format("%a, %d %m %Y %H:%M:%S GMT");
        let last_modified = HeaderValue::from_str(&last_modified.to_string()).unwrap();

        let service_worker = warp::get()
            .and(warp::path("service_worker.js"))
            .and(path::end())
            .map(move || {
                let template = ServiceWorker {
                    static_files: static_files.clone(),
                    static_bin_files: bin_files.clone(),
                };
                warp::http::Response::builder()
                    .header(CONTENT_TYPE, HeaderValue::from_static("text/javascript"))
                    .header(LAST_MODIFIED, last_modified.clone())
                    .body(template.render().unwrap())
                    .unwrap()
            });
        terms_of_use
            .or(about)
            .or(style_guide)
            .or(wordle)
            .or(offline)
            .or(service_worker)
            .debug_boxed()
    };

    let redirects = {
        let favico_redirect = warp::get()
            .and(warp::path("favicon.ico"))
            .map(|| warp::redirect(Uri::from_static("/icons/favicon.ico")));

        let index_redirect = warp::get()
            .and(path::end())
            .map(|| warp::redirect(Uri::from_static("/search")));

        favico_redirect.or(index_redirect).debug_boxed()
    };

    let jaeger_proxy = {
        let base = warp::path!("admin" / "jaeger" / ..);
        let forward_url = "http://127.0.0.1:16686".to_owned();
        let forward =
            proxy::reverse_proxy_filter(String::new(), forward_url).with(warp::trace(|_info| {
                tracing::info_span!("Forward jaeger request")
            }));
        let proxy = with_administrator_auth(db.clone())
            .and(forward)
            .recover(handle_error)
            .with(warp::trace(|info| {
                tracing::info_span!(
                    "Jaeger reverse proxy request",
                    method = %info.method(),
                    path = %info.path(),
                )
            }));

        base.and(proxy).debug_boxed()
    };

    let static_files = warp::fs::dir(cfg.static_site_files.clone())
        .or(warp::fs::dir(cfg.other_static_files.clone()));

    let routes = search
        .or(simple_templates)
        .or(redirects)
        .or(submit(db.clone(), tantivy.clone()))
        .or(moderation(db.clone(), tantivy.clone()))
        .or(details(db.clone()))
        .or(edit(db.clone(), tantivy))
        .or(auth(db.clone(), &cfg).await)
        .or(static_files)
        .recover(handle_error)
        .debug_boxed();

    info!("Visit https://127.0.0.1:{}/", cfg.https_port);

    let http_redirect = path::full()
        .map(move |path: FullPath| {
            let to = Uri::builder()
                .scheme("https")
                .authority("isixhosa.click")
                .path_and_query(path.as_str())
                .build()
                .unwrap();
            warp::redirect(to)
        })
        .with(warp::trace(|info| {
            tracing::info_span!(
                    "HTTP redirect",
                    method = %info.method(),
                    path = %info.path(),
            )
        }));

    let has_reverse_proxy = cfg.cert_path.is_none() || cfg.key_path.is_none();

    if !has_reverse_proxy {
        let http_redirect = warp::serve(http_redirect);
        tokio::spawn(http_redirect.run(([0, 0, 0, 0], cfg.http_port)));
    }

    // Add post filters such as minification, logging, and gzip
    let serve = jaeger_proxy
        .or(wrap_filter!(routes))
        .or(wrap_filter!(with_any_auth(db).map(|auth, _db| {
            reply::with_status(NotFound { auth }, StatusCode::NOT_FOUND).into_response()
        })));

    if has_reverse_proxy {
        warp::serve(serve).run(([0, 0, 0, 0], cfg.http_port)).await
    } else {
        warp::serve(serve)
            .tls()
            .cert_path(cfg.cert_path.unwrap())
            .key_path(cfg.key_path.unwrap())
            .run(([0, 0, 0, 0], cfg.https_port))
            .await
    }
}

#[derive(Deserialize, Clone, Debug)]
struct LiveSearchParams {
    include_own_suggestions: Option<bool>,
}

#[derive(Deserialize, Clone, Debug)]
struct SearchQuery {
    #[serde(alias = "q")]
    query: String,
    #[serde(default = "false_fn")]
    raw: bool,
}

#[derive(Template, Clone, Debug)]
#[template(path = "404.askama.html")]
struct NotFound {
    auth: Auth,
}

#[derive(Template, Clone, Debug)]
#[template(path = "about.askama.html")]
struct About {
    auth: Auth,
    word_count: u64,
}

#[derive(Template, Clone, Debug)]
#[template(path = "terms_of_use.askama.html")]
struct TermsOfUse {
    auth: Auth,
}

#[derive(Template, Clone, Debug)]
#[template(path = "style_guide.askama.html")]
struct StyleGuide {
    auth: Auth,
}

#[derive(Template, Clone, Debug)]
#[template(path = "wordle.askama.html")]
struct Wordle {
    auth: Auth,
}

#[derive(Template, Clone, Debug)]
#[template(path = "service_worker.askama.js", escape = "none", syntax = "js")]
struct ServiceWorker {
    static_files: Vec<String>,
    static_bin_files: Vec<String>,
}

#[derive(Template, Clone, Debug)]
#[template(path = "offline.askama.html")]
struct Offline;

#[derive(Template)]
#[template(path = "search.askama.html")]
struct Search {
    auth: Auth,
    hits: Vec<WordHit>,
    query: String,
}

#[instrument(
    name = "Search with a query string",
    fields(
        query = %query.query,
        raw = %query.raw,
    ),
    skip_all,
)]
async fn query_search(
    query: SearchQuery,
    tantivy: Arc<TantivyClient>,
    auth: Auth,
    _db: impl PublicAccessDb,
) -> Result<impl Reply, Rejection> {
    let results = tantivy
        .search(query.query.clone(), IncludeResults::AcceptedOnly, false)
        .await
        .unwrap();

    if !query.raw {
        let template = Search {
            auth,
            query: query.query,
            hits: results,
        };

        Ok(askama_warp::reply(&template))
    } else {
        Ok(reply::json(&results).into_response())
    }
}

#[derive(Deserialize, Debug)]
struct DuplicateQuery {
    suggestion: NonZeroU64,
}

#[instrument(
    name = "Search for duplicates of a suggestion",
    fields(suggestion_id = %query.suggestion),
    skip_all,
)]
async fn duplicate_search(
    query: DuplicateQuery,
    tantivy: Arc<TantivyClient>,
    _user: FullUser,
    db: impl ModeratorAccessDb,
) -> Result<impl Reply, Rejection> {
    let suggestion = SuggestedWord::fetch_alone(&db, query.suggestion.get());

    let include = IncludeResults::AcceptedAndAllSuggestions;
    let res = match suggestion.filter(|w| w.word_id.is_none()) {
        Some(w) => {
            let english = tantivy
                .search(w.english.current().clone(), include, true)
                .await
                .unwrap();
            let xhosa = tantivy
                .search(w.xhosa.current().clone(), include, true)
                .await
                .unwrap();

            let mut results = HashSet::with_capacity(english.len() + xhosa.len());
            results.extend(english);
            results.extend(xhosa);
            // Exclude this suggestion and the original of this suggestion (the word being edited)
            results.retain(|res| {
                let is_this_suggestion = res.id == query.suggestion.get() && res.is_suggestion;
                let is_original = Some(res.id) == w.word_id && !res.is_suggestion;
                dbg!(res, is_this_suggestion, is_original);
                !(is_this_suggestion || is_original)
            });
            results
        }
        None => HashSet::new(),
    };

    Ok(reply::json(&res))
}

#[instrument(
    name = "Begin live search websocket connection",
    fields(include_own_suggestions = %params.include_own_suggestions.unwrap_or_default()),
    skip_all,
)]
fn live_search(
    ws: warp::ws::Ws,
    tantivy: Arc<TantivyClient>,
    params: LiveSearchParams,
    auth: Auth,
    _db: impl PublicAccessDb,
) -> impl Reply {
    ws.on_upgrade(move |websocket| {
        let (sender, stream) = websocket.split();
        let include_suggestions_from_user = if params.include_own_suggestions.unwrap_or(false) {
            auth.user_id()
        } else {
            None
        };

        let actor = LiveSearchSession::new(
            sender,
            tantivy,
            include_suggestions_from_user,
            auth.has_permissions(Permissions::Moderator),
        );

        let addr = xtra::spawn_tokio(actor, Mailbox::bounded(4));

        tokio::spawn(stream.map(Ok).forward(addr.into_sink()));
        futures::future::ready(())
    })
}

fn spawn_send_interval<A, M>(addr: WeakAddress<A>, interval: Duration, msg: M)
where
    A: Handler<M>,
    M: Clone + Send + Sync + 'static,
{
    let addr_clone = addr.clone();
    let fut = async move {
        let mut interval = tokio::time::interval(interval);
        loop {
            interval.tick().await;
            if addr.send(msg.clone()).await.is_err() {
                return;
            }
        }
    };
    tokio::spawn(xtra::scoped(&addr_clone, fut));
}
