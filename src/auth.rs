use std::{convert::Infallible, sync::Arc};

use crate::auth::db_impl::DbImpl;
use crate::format::{DisplayHtml, HtmlFormatter};
use crate::serialization::{deserialize_checkbox, false_fn, qs_form};
use crate::Config;
use askama::Template;
use cookie::time::OffsetDateTime;
use cookie::{Cookie, Expiration};
use dashmap::DashMap;
use openid::{Client, Discovered, DiscoveredClient, Options, StandardClaims, Token, Userinfo};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rand::Rng;
use rusqlite::{params, Row};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::convert::TryFrom;
use std::fmt::{Debug, Display, Formatter};
use std::num::NonZeroU64;
use std::str::FromStr;
use std::time::{Duration, Instant};
use warp::http::uri;
use warp::path::FullPath;
use warp::{
    http::{Response, StatusCode},
    reject, Filter, Rejection, Reply,
};
use fallible_iterator::FallibleIterator;

type OpenIDClient = Client<Discovered, StandardClaims>;

lazy_static::lazy_static! {
    pub static ref IN_PROGRESS_SIGN_INS: DashMap<SignInSessionId, SignInState> = DashMap::new();
    pub static ref MAX_OIDC_AGE: chrono::Duration = chrono::Duration::minutes(20);
}

const STAY_LOGGED_IN_COOKIE: &str = "isixhosa_click_login_token";
const SIGN_IN_SESSION_ID: &str = "isixhosa_click_sign_in_session";

async fn sweep_in_progress_sign_ins() {
    const TEN_MINUTES: Duration = Duration::from_secs(10 * 60);

    loop {
        tokio::time::sleep(TEN_MINUTES).await;
        let now = Instant::now();

        IN_PROGRESS_SIGN_INS.retain(|_session_id, state| now - *state.last_change() < TEN_MINUTES);
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct SignInSessionId(String);

impl FromStr for SignInSessionId {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SignInSessionId(s.to_owned()))
    }
}

#[derive(Deserialize, Clone, Debug, Hash, Eq, PartialEq)]
pub struct StaySignedInToken {
    pub token: String,
    pub token_id: u64,
}

impl FromStr for StaySignedInToken {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Default, Clone, Debug)]
pub struct Auth {
    user: Option<User>,
}

impl From<User> for Auth {
    fn from(user: User) -> Self {
        Auth { user: Some(user) }
    }
}

impl Auth {
    pub fn has_permissions(&self, permissions: Permissions) -> bool {
        match self.user.as_ref() {
            Some(user) => user.permissions.contains(permissions),
            None => false,
        }
    }

    pub fn username(&self) -> Option<&str> {
        self.user.as_ref().map(|user| &user.username as &str)
    }

    pub fn user_id(&self) -> Option<NonZeroU64> {
        self.user.as_ref().map(|u| u.id)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Permissions {
    User,
    Moderator,
}

impl Permissions {
    pub fn contains(&self, other: Permissions) -> bool {
        *self == Permissions::Moderator || other == Permissions::User
    }
}

#[derive(Deserialize, Debug)]
pub struct LoginRedirectQuery {
    redirect: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct OpenIdLoginQuery {
    pub code: String,
    pub state: Option<String>,
}

pub enum SignInState {
    WaitingForOpenIdResponse {
        state_change: Instant,
        csrf_token: String,
        nonce: String,
    },
    WaitingForSignUp {
        userinfo: Box<Userinfo>,
        state_change: Instant,
    },
}

impl SignInState {
    fn last_change(&self) -> &Instant {
        match self {
            SignInState::WaitingForOpenIdResponse { state_change, .. } => state_change,
            SignInState::WaitingForSignUp { state_change, .. } => state_change,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OpenIdState {
    pub redirect: Option<String>,
    pub csrf_token: String,
}

#[derive(Deserialize, Debug)]
pub struct SignupForm {
    username: String,
    #[serde(default = "false_fn")]
    #[serde(deserialize_with = "deserialize_checkbox")]
    dont_display_name: bool,
    #[serde(default = "false_fn")]
    #[serde(deserialize_with = "deserialize_checkbox")]
    license_agree: bool,
    #[serde(default = "false_fn")]
    #[serde(deserialize_with = "deserialize_checkbox")]
    tou_agree: bool,
    #[serde(flatten)]
    openid_query: OpenIdLoginQuery,
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: NonZeroU64,
    pub username: String,
    pub display_name: bool,
    pub advanced_submit_form: bool,
    pub email: String,
    pub permissions: Permissions,
    pub locked: bool,
}

#[derive(Clone, Debug)]
pub struct PublicUserInfo {
    pub id: NonZeroU64,
    pub username: String,
    pub display_name: bool,
}

impl PublicUserInfo {
    pub fn fetch_public_contributors_for_word(
        db: &impl PublicAccessDb,
        word: u64,
    ) -> Vec<PublicUserInfo> {
        const SELECT: &str = "
            SELECT users.user_id as suggesting_user, users.username, display_name
            FROM user_attributions
            INNER JOIN users ON users.user_id = user_attributions.user_id
            WHERE word_id = ?1 AND users.display_name = 1;
        ";

        let conn = db.get().unwrap();

        let mut query = conn
            .prepare(SELECT)
            .unwrap();

        query
            .query(params![word])
            .unwrap()
            .map(|row| PublicUserInfo::try_from(row))
            .collect()
            .unwrap()
    }
}

impl TryFrom<&Row<'_>> for PublicUserInfo {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(PublicUserInfo {
            id: NonZeroU64::new(row.get::<&str, u64>("suggesting_user")?).unwrap(),
            username: row.get("username")?,
            display_name: row.get("display_name")?,
        })
    }
}

impl DisplayHtml for PublicUserInfo {
    fn fmt(&self, f: &mut HtmlFormatter) -> std::fmt::Result {
        f.write_text(
            Some(&self.username[..])
                .filter(|_| self.display_name)
                .unwrap_or_default(),
        )
    }

    fn is_empty_str(&self) -> bool {
        !self.display_name
    }
}

#[derive(Template)]
#[template(path = "sign_up.askama.html")]
struct SignUpTemplate {
    auth: Auth,
    openid_query: OpenIdLoginQuery,
    previous_failure: Option<SignUpFailure>,
}

enum SignUpFailure {
    InvalidUsername,
    DidNotAgree,
    NoEmail,
}

pub async fn auth(
    db: DbBase,
    cfg: &Config,
) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    let redirect = Config::host_builder(&cfg.host, cfg.https_port)
        .path_and_query("/login/oauth2/code/oidc")
        .build()
        .unwrap()
        .to_string();

    let issuer = reqwest::Url::parse("https://accounts.google.com").unwrap();

    let client = Arc::new(
        DiscoveredClient::discover(
            cfg.oidc_client.clone(),
            cfg.oidc_secret.clone(),
            Some(redirect),
            issuer,
        )
        .await
        .unwrap(),
    );

    let (host, https_port) = (cfg.host.clone(), cfg.https_port);
    let with_client_host = warp::any()
        .map(move || (client.clone(), Config::host_builder(&host, https_port)))
        .untuple_one();

    tokio::task::spawn(sweep_in_progress_sign_ins());

    let login = warp::path!("login" / "oauth2" / "authorization" / "oidc")
        .and(warp::get())
        .and(with_client_host.clone())
        .and(warp::query::<LoginRedirectQuery>())
        .and_then(reply_authorize);

    let oidc_code = warp::path!("login" / "oauth2" / "code" / "oidc")
        .and(warp::get())
        .and(with_any_auth(db.clone()))
        .and(with_session())
        .and(with_client_host.clone())
        .and(warp::query::<OpenIdLoginQuery>())
        .and_then(reply_login);

    let logout = warp::get()
        .and(warp::path("logout"))
        .and(warp::path::end())
        .and(with_user_auth(db.clone()))
        .and(warp::cookie::cookie(STAY_LOGGED_IN_COOKIE))
        .and_then(reply_logout);

    let sign_up = warp::post()
        .and(warp::path("signup"))
        .and(warp::path::end())
        .and(warp::body::content_length_limit(64 * 1024))
        .and(with_any_auth(db))
        .and(with_session())
        .and(with_client_host)
        .and(qs_form())
        .and_then(signup_form_submit);

    login.or(oidc_code).or(sign_up).or(logout).boxed()
}

fn with_session() -> impl Filter<Extract = (SignInSessionId,), Error = Rejection> + Clone {
    warp::cookie(SIGN_IN_SESSION_ID).and_then(|id| async {
        if IN_PROGRESS_SIGN_INS.contains_key(&id) {
            Ok(id)
        } else {
            Err(warp::reject::custom(Unauthorized {
                redirect: String::new(),
                reason: UnauthorizedReason::InvalidCookie,
            }))
        }
    })
}

pub fn random_string_token() -> String {
    let mut bytes = [0; 32];
    rand::thread_rng().fill(&mut bytes[..]);
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

async fn reply_authorize(
    oidc_client: Arc<OpenIDClient>,
    host_uri_builder: uri::Builder,
    redirect: LoginRedirectQuery,
) -> Result<impl warp::Reply, Infallible> {
    let redirect = redirect
        .redirect
        .and_then(|path| host_uri_builder.path_and_query(path).build().ok())
        .map(|uri| uri.to_string());
    let state = OpenIdState {
        redirect,
        csrf_token: random_string_token(),
    };

    let session_id = SignInSessionId(random_string_token());
    let nonce = random_string_token();

    let auth_url = oidc_client.auth_url(&Options {
        scope: Some("openid email".into()),
        state: Some(serde_json::to_string(&state).unwrap()),
        nonce: Some(nonce.clone()),
        max_age: Some(*MAX_OIDC_AGE),
        ..Default::default()
    });

    let session_id_cookie = Cookie::build(SIGN_IN_SESSION_ID, &session_id.0)
        .path("/")
        .http_only(true)
        .secure(true)
        .expires(Expiration::Session)
        .finish()
        .to_string();

    IN_PROGRESS_SIGN_INS.insert(
        session_id,
        SignInState::WaitingForOpenIdResponse {
            state_change: Instant::now(),
            csrf_token: state.csrf_token,
            nonce,
        },
    );

    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(warp::http::header::LOCATION, auth_url.to_string())
        .header(warp::http::header::SET_COOKIE, session_id_cookie)
        .body("")
        .unwrap())
}

async fn request_token(
    oidc_client: Arc<OpenIDClient>,
    session_id: &SignInSessionId,
    openid_query: &OpenIdLoginQuery,
) -> anyhow::Result<Option<(Token, Userinfo)>> {
    #[derive(Debug)]
    enum SignInInvalid {
        SignInState,
        SignInId,
        CsrfToken,
    }

    impl Display for SignInInvalid {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            Debug::fmt(self, f)
        }
    }

    impl std::error::Error for SignInInvalid {}

    let (csrf_token, nonce) = match IN_PROGRESS_SIGN_INS.get(session_id).as_deref() {
        Some(SignInState::WaitingForOpenIdResponse {
            csrf_token, nonce, ..
        }) => (csrf_token.clone(), nonce.clone()),
        Some(_) => return Err(SignInInvalid::SignInState.into()),
        None => return Err(SignInInvalid::SignInId.into()),
    };

    let state: OpenIdState = serde_json::from_str(
        openid_query
            .state
            .as_ref()
            .ok_or(SignInInvalid::CsrfToken)?,
    )?;

    if state.csrf_token != csrf_token {
        return Err(SignInInvalid::CsrfToken.into());
    }

    let mut token: Token = oidc_client.request_token(&openid_query.code).await?.into();

    if let Some(mut id_token) = token.id_token.as_mut() {
        oidc_client.decode_token(&mut id_token)?;
        oidc_client.validate_token(id_token, Some(&nonce), None)?;
    } else {
        return Ok(None);
    }

    let userinfo = oidc_client.request_userinfo(&token).await?;

    Ok(Some((token, userinfo)))
}

async fn reply_login(
    _auth: Auth,
    db: impl PublicAccessDb,
    session_id: SignInSessionId,
    oidc_client: Arc<OpenIDClient>,
    host_builder: uri::Builder,
    openid_query: OpenIdLoginQuery,
) -> Result<impl warp::Reply, Infallible> {
    let request_token = request_token(oidc_client, &session_id, &openid_query).await;

    let (token, oidc_id, userinfo) = match request_token {
        Ok(Some((token, user_info))) => match user_info.sub {
            Some(ref id) => (token, id.clone(), user_info),
            None => {
                log::error!("Error requesting token: no remote `sub` id found");

                return Ok(Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body("")
                    .unwrap()
                    .into_response());
            }
        },
        Ok(None) => {
            log::error!("Error requesting token during login: no id_token found");

            return Ok(Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body("")
                .unwrap()
                .into_response());
        }
        Err(err) => {
            log::error!("Error requesting token during login: {:#?}", err);

            return Ok(Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body("")
                .unwrap()
                .into_response());
        }
    };

    let state: OpenIdState = serde_json::from_str(openid_query.state.as_ref().unwrap()).unwrap();

    let response = match User::fetch_by_oidc_id(&db, token, oidc_id) {
        Some(user) => reply_insert_session(db, session_id, user, host_builder, state.redirect)
            .await
            .into_response(),
        None => {
            IN_PROGRESS_SIGN_INS.insert(
                session_id,
                SignInState::WaitingForSignUp {
                    userinfo: Box::new(userinfo),
                    state_change: Instant::now(),
                },
            );

            SignUpTemplate {
                auth: Default::default(),
                openid_query,
                previous_failure: None,
            }
            .into_response()
        }
    };

    Ok(response)
}

async fn reply_insert_session(
    db: impl PublicAccessDb,
    session_id: SignInSessionId,
    user: User,
    host_uri_builder: uri::Builder,
    redirect_url: Option<String>,
) -> impl warp::Reply {
    const SIX_MONTHS: Duration = Duration::from_secs(60 * 60 * 24 * 31 * 6);

    let redirect_url = redirect_url.unwrap_or_else(|| {
        host_uri_builder
            .path_and_query("")
            .build()
            .unwrap()
            .to_string()
    });

    let token = tokio::task::spawn_blocking(move || StaySignedInToken::new(&db, user.id.get()))
        .await
        .unwrap();
    let authorization_cookie = Cookie::build(
        STAY_LOGGED_IN_COOKIE,
        serde_json::to_string(&(token.token, token.token_id)).unwrap(),
    )
    .path("/")
    .http_only(true)
    .secure(true)
    .expires(OffsetDateTime::now_utc() + SIX_MONTHS)
    .finish()
    .to_string();

    IN_PROGRESS_SIGN_INS.remove(&session_id);

    Response::builder()
        .status(StatusCode::FOUND)
        .header(warp::http::header::LOCATION, redirect_url)
        .header(warp::http::header::SET_COOKIE, authorization_cookie)
        .body("")
        .unwrap()
}

async fn signup_form_submit(
    _auth: Auth,
    db: impl PublicAccessDb,
    session_id: SignInSessionId,
    _oidc_client: Arc<OpenIDClient>,
    host_uri_builder: uri::Builder,
    form: SignupForm,
) -> Result<impl warp::Reply, Infallible> {
    let userinfo = match IN_PROGRESS_SIGN_INS.get(&session_id).as_deref() {
        Some(SignInState::WaitingForSignUp { userinfo, .. }) => userinfo.clone(),
        _ => return Ok(warp::reply::with_status("", StatusCode::FORBIDDEN).into_response()),
    };

    if !form.tou_agree || !form.license_agree {
        return Ok(SignUpTemplate {
            auth: Default::default(),
            openid_query: form.openid_query,
            previous_failure: Some(SignUpFailure::DidNotAgree),
        }
        .into_response());
    }

    if !form
        .username
        .chars()
        .all(|c| c.is_alphanumeric() || [' ', '-', '_'].contains(&c))
        || !(2..=128usize).contains(&form.username.len())
    {
        return Ok(SignUpTemplate {
            auth: Default::default(),
            openid_query: form.openid_query,
            previous_failure: Some(SignUpFailure::InvalidUsername),
        }
        .into_response());
    }

    let email = match userinfo.email.clone().filter(|_| userinfo.email_verified) {
        Some(email) => email,
        None => {
            return Ok(SignUpTemplate {
                auth: Default::default(),
                openid_query: form.openid_query,
                previous_failure: Some(SignUpFailure::NoEmail),
            }
            .into_response())
        }
    };

    let state: OpenIdState =
        serde_json::from_str(form.openid_query.state.as_ref().unwrap()).unwrap();
    let redirect_url = state.redirect.clone();

    let db_clone = db.clone();
    let user = tokio::task::spawn_blocking(move || {
        User::register(
            &db,
            userinfo,
            form.username,
            !form.dont_display_name,
            false,
            email,
            Permissions::User,
        )
    })
    .await
    .unwrap();

    Ok(
        reply_insert_session(db_clone, session_id, user, host_uri_builder, redirect_url)
            .await
            .into_response(),
    )
}

async fn reply_logout(
    _user: User,
    db: impl UserAccessDb,
    token: StaySignedInToken,
) -> Result<impl warp::Reply, Infallible> {
    let deleted_cookie = Cookie::build(STAY_LOGGED_IN_COOKIE, "")
        .path("/")
        .http_only(true)
        .secure(true)
        .expires(OffsetDateTime::now_utc())
        .finish()
        .to_string();

    tokio::task::spawn_blocking(move || token.delete(&db));

    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(warp::http::header::LOCATION, "/search")
        .header(warp::http::header::SET_COOKIE, deleted_cookie)
        .body("")
        .unwrap())
}

#[derive(Debug)]
pub struct Unauthorized {
    pub reason: UnauthorizedReason,
    pub redirect: String,
}

#[derive(Debug)]
pub enum UnauthorizedReason {
    NotLoggedIn,
    NoPermissions,
    /// During sign in process if the sign in session cookie is not valid
    InvalidCookie,
    /// User account locked
    Locked,
}

impl reject::Reject for Unauthorized {}

async fn extract_user(
    db: impl PublicAccessDb,
    redirect: String,
    stay_signed_in: Option<StaySignedInToken>,
) -> Result<User, Rejection> {
    tokio::task::spawn_blocking(move || {
        if let Some(stay_signed_in) = stay_signed_in {
            if let Some(user) = stay_signed_in.verify_token(&db) {
                let user = User::fetch_by_id(&db, user).unwrap();
                if !user.locked {
                    Ok(user)
                } else {
                    Err(warp::reject::custom(Unauthorized {
                        reason: UnauthorizedReason::Locked,
                        redirect,
                    }))
                }
            } else {
                Err(warp::reject::custom(Unauthorized {
                    reason: UnauthorizedReason::NotLoggedIn,
                    redirect,
                }))
            }
        } else {
            Err(warp::reject::custom(Unauthorized {
                reason: UnauthorizedReason::NotLoggedIn,
                redirect,
            }))
        }
    })
    .await
    .unwrap()
}

#[derive(Clone)]
pub struct DbBase(Pool<SqliteConnectionManager>);

impl DbBase {
    pub fn new(pool: Pool<SqliteConnectionManager>) -> DbBase {
        DbBase(pool)
    }
}

mod db_impl {
    use super::*;

    #[derive(Clone)]
    pub(super) struct DbImpl(pub(super) Pool<SqliteConnectionManager>);

    impl PublicAccessDb for DbImpl {
        fn get(&self) -> Result<PooledConnection<SqliteConnectionManager>, r2d2::Error> {
            self.0.get()
        }
    }

    impl UserAccessDb for DbImpl {}
    impl ModeratorAccessDb for DbImpl {}
}

pub trait PublicAccessDb: Clone + Send + Sync + 'static {
    fn get(&self) -> Result<PooledConnection<SqliteConnectionManager>, r2d2::Error>;
}

pub trait UserAccessDb: PublicAccessDb {}
pub trait ModeratorAccessDb: UserAccessDb {}

pub fn with_any_auth(
    db: DbBase,
) -> impl Filter<Extract = (Auth, impl PublicAccessDb), Error = Infallible> + Clone {
    let db_clone = db.clone();
    warp::path::full()
        .map(|path: FullPath| path.as_str().to_owned())
        .and(warp::cookie::optional(STAY_LOGGED_IN_COOKIE))
        .and_then(move |path, cookie| extract_user(DbImpl(db.0.clone()), path, cookie))
        .map(|user| Auth { user: Some(user) })
        .or(warp::any().map(Auth::default))
        .unify()
        .and(warp::any().map(move || DbImpl(db_clone.0.clone())))
}

pub fn with_user_auth(
    db: DbBase,
) -> impl Filter<Extract = (User, impl UserAccessDb), Error = Rejection> + Clone {
    let db_clone = db.clone();
    warp::path::full()
        .map(|path: FullPath| path.as_str().to_owned())
        .and(warp::cookie::optional(STAY_LOGGED_IN_COOKIE))
        .and_then(move |path, cookie| extract_user(DbImpl(db.0.clone()), path, cookie))
        .and(warp::any().map(move || DbImpl(db_clone.0.clone())))
}

pub fn with_moderator_auth(
    db: DbBase,
) -> impl Filter<Extract = (User, impl ModeratorAccessDb), Error = Rejection> + Clone {
    let db_clone = db.clone();
    warp::path::full()
        .map(|path: FullPath| path.as_str().to_owned())
        .and(warp::cookie::optional(STAY_LOGGED_IN_COOKIE))
        .and(warp::any().map(move || db.clone()))
        .and_then(|redirect: String, token, db: DbBase| async move {
            match extract_user(DbImpl(db.0.clone()), redirect.clone(), token).await {
                Ok(user) if user.permissions.contains(Permissions::Moderator) => Ok(user),
                Ok(_unauthorized) => Err(warp::reject::custom(Unauthorized {
                    reason: UnauthorizedReason::NoPermissions,
                    redirect,
                })),
                Err(e) => Err(e),
            }
        })
        .and(warp::any().map(move || DbImpl(db_clone.0.clone())))
}
