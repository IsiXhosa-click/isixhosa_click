use crate::serialization::{deserialize_checkbox, false_fn, qs_form};
use crate::{spawn_blocking_child, Config, DebugBoxedExt, DebugExt};
use askama::Template;
use cookie::time::OffsetDateTime;
use cookie::{Cookie, Expiration};
use dashmap::DashMap;
use isixhosa_common::auth::{Auth, Permissions};
use isixhosa_common::database::db_impl::DbImpl;
use isixhosa_common::database::{DbBase, ModeratorAccessDb, PublicAccessDb, UserAccessDb};
use openid::{Client, Discovered, DiscoveredClient, Options, StandardClaims, Token, Userinfo};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::convert::Infallible;
use std::fmt::{Debug, Display, Formatter};
use std::num::NonZeroU64;
use std::str::FromStr;
use std::time::{Duration, Instant};
use tracing::{debug, error, instrument, Span};
use url::Url;
use warp::http::uri;
use warp::path::FullPath;
use warp::{
    http::{Response, StatusCode},
    reject, Filter, Rejection, Reply,
};
use xtra::spawn::TokioGlobalSpawnExt;
use xtra::{Actor, Address, Context, Handler, Message};

type OpenIDClient = Address<OidcActor>;

lazy_static::lazy_static! {
    pub static ref IN_PROGRESS_SIGN_INS: DashMap<SignInSessionId, SignInState> = DashMap::new();
    pub static ref MAX_OIDC_AGE: chrono::Duration = chrono::Duration::minutes(20);
}

struct OidcActor {
    client: Client<Discovered, StandardClaims>,
    client_id: String,
    client_secret: String,
    redirect: String,
    issuer: Url,
}

impl OidcActor {
    async fn new_client(
        client_id: &str,
        client_secret: &str,
        redirect: &str,
        issuer: &Url,
    ) -> Client<Discovered, StandardClaims> {
        DiscoveredClient::discover(
            client_id.to_owned(),
            client_secret.to_owned(),
            Some(redirect.to_owned()),
            issuer.clone(),
        )
        .await
        .unwrap()
    }

    async fn new(client_id: String, client_secret: String, redirect: String, issuer: Url) -> Self {
        Self {
            client: Self::new_client(&client_id, &client_secret, &redirect, &issuer).await,
            client_id,
            client_secret,
            redirect,
            issuer,
        }
    }
}

#[async_trait::async_trait]
impl Actor for OidcActor {
    async fn started(&mut self, ctx: &mut Context<Self>) {
        const INTERVAL: Duration = Duration::from_secs(60 * 60 * 24); // 24 hours
        tokio::spawn(ctx.notify_interval(INTERVAL, || RefreshClient).unwrap());
    }
}

struct RefreshClient;

impl Message for RefreshClient {
    type Result = ();
}

#[async_trait::async_trait]
impl Handler<RefreshClient> for OidcActor {
    async fn handle(&mut self, _msg: RefreshClient, _ctx: &mut Context<Self>) {
        self.client = Self::new_client(
            &self.client_id,
            &self.client_secret,
            &self.redirect,
            &self.issuer,
        )
        .await;
    }
}

struct GetAuthUrl(Options);

impl Message for GetAuthUrl {
    type Result = Url;
}

#[async_trait::async_trait]
impl Handler<GetAuthUrl> for OidcActor {
    async fn handle(&mut self, options: GetAuthUrl, _: &mut Context<Self>) -> Url {
        self.client.auth_url(&options.0)
    }
}

struct RequestToken {
    code: String,
    nonce: String,
}

impl Message for RequestToken {
    type Result = anyhow::Result<Option<(Token, Userinfo)>>;
}

#[async_trait::async_trait]
impl Handler<RequestToken> for OidcActor {
    async fn handle(
        &mut self,
        req: RequestToken,
        _: &mut Context<Self>,
    ) -> anyhow::Result<Option<(Token, Userinfo)>> {
        let mut token: Token = self.client.request_token(&req.code).await?.into();

        if let Some(id_token) = token.id_token.as_mut() {
            self.client.decode_token(id_token)?;
            self.client
                .validate_token(id_token, Some(&req.nonce), None)?;
        } else {
            return Ok(None);
        }

        let userinfo = self.client.request_userinfo(&token).await?;

        Ok(Some((token, userinfo)))
    }
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

#[derive(Deserialize, Clone, Hash, Eq, PartialEq)]
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

// TODO this is probably passed around a bit too much?
#[derive(Debug, Clone)]
pub struct FullUser {
    pub id: NonZeroU64,
    pub username: String,
    pub display_name: bool,
    pub email: String,
    pub permissions: Permissions,
    pub locked: bool,
}

impl Into<isixhosa_common::auth::User> for FullUser {
    fn into(self) -> isixhosa_common::auth::User {
        isixhosa_common::auth::User {
            user_id: self.id,
            username: self.username,
            permissions: self.permissions,
        }
    }
}

impl Into<Auth> for FullUser {
    fn into(self) -> Auth {
        let user: isixhosa_common::auth::User = self.into();
        user.into()
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
) -> impl Filter<Error = Rejection, Extract = impl Reply> + Clone {
    let redirect = Config::host_builder(&cfg.host, cfg.https_port)
        .path_and_query("/login/oauth2/code/oidc")
        .build()
        .unwrap()
        .to_string();

    let issuer = url::Url::parse("https://accounts.google.com").unwrap();

    let client = OidcActor::new(
        cfg.oidc_client.clone(),
        cfg.oidc_secret.clone(),
        redirect,
        issuer,
    )
    .await
    .create(Some(32))
    .spawn_global();

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
        .and(warp::query::<OpenIdLoginQuery>())
        .and(with_session())
        .and(with_client_host.clone())
        .and(with_public_db(db.clone()))
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
        .and(qs_form())
        .and(with_session())
        .and(with_client_host)
        .and(with_public_db(db.clone()))
        .and_then(signup_form_submit);

    login.or(oidc_code).or(sign_up).or(logout).debug_boxed()
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
    oidc_client: OpenIDClient,
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

    let auth_url = oidc_client
        .send(GetAuthUrl(Options {
            scope: Some("openid email".into()),
            state: Some(serde_json::to_string(&state).unwrap()),
            nonce: Some(nonce.clone()),
            max_age: Some(*MAX_OIDC_AGE),
            ..Default::default()
        }))
        .await
        .unwrap();

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
    oidc_client: OpenIDClient,
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

    let res = oidc_client
        .send(RequestToken {
            code: openid_query.code.clone(),
            nonce,
        })
        .await
        .unwrap();

    res
}

async fn reply_login(
    openid_query: OpenIdLoginQuery,
    session_id: SignInSessionId,
    oidc_client: OpenIDClient,
    host_builder: uri::Builder,
    db: impl PublicAccessDb,
) -> Result<impl warp::Reply, Infallible> {
    let request_token = request_token(oidc_client, &session_id, &openid_query).await;
    let mk_err = || {
        IN_PROGRESS_SIGN_INS.remove(&session_id);

        Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body("")
            .unwrap()
            .into_response())
    };

    let (token, oidc_id, userinfo) = match request_token {
        Ok(Some((token, user_info))) => match user_info.sub {
            Some(ref id) => (token, id.clone(), user_info),
            None => {
                error!("Error requesting token: no remote `sub` id found");
                return mk_err();
            }
        },
        Ok(None) => {
            error!("Error requesting token during login: no id_token found");
            return mk_err();
        }
        Err(err) => {
            error!("Error requesting token during login: {:#?}", err);
            return mk_err();
        }
    };

    let state: OpenIdState = serde_json::from_str(openid_query.state.as_ref().unwrap()).unwrap();

    let response = match FullUser::fetch_by_oidc_id(&db, token, oidc_id) {
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
    user: FullUser,
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

    let token = spawn_blocking_child(move || StaySignedInToken::new(&db, user.id.get()))
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
    form: SignupForm,
    session_id: SignInSessionId,
    _oidc_client: OpenIDClient,
    host_uri_builder: uri::Builder,
    db: impl PublicAccessDb,
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
    let user = spawn_blocking_child(move || {
        FullUser::register(
            &db,
            userinfo,
            form.username,
            !form.dont_display_name,
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
    _user: FullUser, // This _user is important as it implicitly validates the given token
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

    spawn_blocking_child(move || token.delete(&db));

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

#[instrument(
    name = "Try to extract user from a token",
    fields(fail_reason, success, id, name),
    skip_all
)]
async fn extract_user(
    db: impl PublicAccessDb,
    redirect: String,
    stay_signed_in: Option<StaySignedInToken>,
) -> Result<FullUser, Rejection> {
    spawn_blocking_child(move || {
        let span = Span::current();

        if let Some(stay_signed_in) = stay_signed_in {
            if let Some(user) = stay_signed_in.verify_token(&db) {
                let user = FullUser::fetch_by_id(&db, user).unwrap();

                span.record("id", &user.id);
                span.record("name", &user.username.as_str());

                if !user.locked {
                    span.record("success", &true);
                    debug!("User successfully authenticated");
                    Ok(user)
                } else {
                    let reason = UnauthorizedReason::Locked;
                    span.record("success", &false);
                    span.record("fail_reason", &reason.to_debug().as_str());
                    debug!("User locked");

                    Err(warp::reject::custom(Unauthorized { reason, redirect }))
                }
            } else {
                let reason = UnauthorizedReason::NotLoggedIn;
                span.record("success", &false);
                span.record("fail_reason", &reason.to_debug().as_str());
                debug!("Invalid token");

                Err(warp::reject::custom(Unauthorized { reason, redirect }))
            }
        } else {
            let reason = UnauthorizedReason::NotLoggedIn;
            span.record("success", &false);
            span.record("fail_reason", &reason.to_debug().as_str());
            debug!("No token");

            Err(warp::reject::custom(Unauthorized { reason, redirect }))
        }
    })
    .await
    .unwrap()
}

fn with_public_db(
    db: DbBase,
) -> impl Filter<Extract = (impl PublicAccessDb,), Error = Infallible> + Clone {
    warp::any().map(move || DbImpl(db.0.clone()))
}

pub fn with_any_auth(
    db: DbBase,
) -> impl Filter<Extract = (Auth, impl PublicAccessDb), Error = Infallible> + Clone {
    let db_clone = db.clone();
    warp::path::full()
        .map(|path: FullPath| path.as_str().to_owned())
        .and(warp::cookie::optional(STAY_LOGGED_IN_COOKIE))
        .and_then(move |path, cookie| extract_user(DbImpl(db.0.clone()), path, cookie))
        .map(|user: FullUser| user.into())
        .or(warp::any().map(Auth::default))
        .unify()
        .and(warp::any().map(move || DbImpl(db_clone.0.clone())))
}

pub fn with_user_auth(
    db: DbBase,
) -> impl Filter<Extract = (FullUser, impl UserAccessDb), Error = Rejection> + Clone {
    let db_clone = db.clone();
    warp::path::full()
        .map(|path: FullPath| path.as_str().to_owned())
        .and(warp::cookie::optional(STAY_LOGGED_IN_COOKIE))
        .and_then(move |path, cookie| extract_user(DbImpl(db.0.clone()), path, cookie))
        .and(warp::any().map(move || DbImpl(db_clone.0.clone())))
}

pub fn with_moderator_auth(
    db: DbBase,
) -> impl Filter<Extract = (FullUser, impl ModeratorAccessDb), Error = Rejection> + Clone {
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

pub fn with_administrator_auth(db: DbBase) -> impl Filter<Extract = (), Error = Rejection> + Clone {
    warp::path::full()
        .map(|path: FullPath| path.as_str().to_owned())
        .and(warp::cookie::optional(STAY_LOGGED_IN_COOKIE))
        .and(warp::any().map(move || db.clone()))
        .and_then(|redirect: String, token, db: DbBase| async move {
            match extract_user(DbImpl(db.0.clone()), redirect.clone(), token).await {
                Ok(user) if user.permissions.contains(Permissions::Administrator) => Ok(()),
                Ok(_unauthorized) => Err(warp::reject::custom(Unauthorized {
                    reason: UnauthorizedReason::NoPermissions,
                    redirect,
                })),
                Err(e) => Err(e),
            }
        })
        .untuple_one()
}
