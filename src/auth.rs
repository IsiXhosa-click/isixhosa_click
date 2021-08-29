use std::{collections::HashMap, convert::Infallible, sync::Arc};

use crate::auth::db_impl::DbImpl;
use crate::Config;
use cookie::time::OffsetDateTime;
use cookie::Cookie;
use log::{error, info};
use openid::{Client, Discovered, DiscoveredClient, Options, StandardClaims, Token, Userinfo};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use serde::Deserialize;
use tokio::sync::RwLock;
use warp::{
    http::{Response, StatusCode},
    reject, Filter, Rejection, Reply,
};

type OpenIDClient = Client<Discovered, StandardClaims>;

// TODO dashmap
lazy_static::lazy_static! {
    pub static ref SESSIONS: Arc<RwLock<Sessions>> = Arc::new(RwLock::new(Sessions::default()));
}

const COOKIE: &str = "isixhosa_click_login_token";

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
pub struct LoginQuery {
    pub code: String,
    pub state: Option<String>,
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub login: Option<String>,
    pub first_name: Option<String>,
    pub email: Option<String>,
    pub activated: bool,
    pub permissions: Permissions,
}

#[derive(Default)]
pub struct Sessions {
    map: HashMap<String, (User, Token, Userinfo)>,
}

pub async fn auth(cfg: &Config) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    let redirect = cfg
        .host_builder()
        .path_and_query("/login/oauth2/code/oidc")
        .build()
        .unwrap()
        .to_string();

    let host_uri = cfg.host_builder().path_and_query("").build().unwrap();

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

    let with_client_host = warp::any()
        .map(move || (client.clone(), host_uri.to_string()))
        .untuple_one();

    let authorize = warp::path!("login" / "oauth2" / "authorization" / "oidc")
        .and(warp::get())
        .and(with_client_host.clone())
        .and_then(reply_authorize);

    let logout = warp::get()
        .and(warp::path("logout"))
        .and(warp::path::end())
        .and(warp::cookie::cookie(COOKIE))
        .and_then(reply_logout);

    let oidc_code = warp::path!("login" / "oauth2" / "code" / "oidc")
        .and(warp::get())
        .and(with_client_host)
        .and(warp::query::<LoginQuery>())
        .and_then(reply_login);

    authorize.or(oidc_code).or(logout)
}

async fn request_token(
    oidc_client: Arc<OpenIDClient>,
    login_query: &LoginQuery,
) -> anyhow::Result<Option<(Token, Userinfo)>> {
    let mut token: Token = oidc_client.request_token(&login_query.code).await?.into();

    if let Some(mut id_token) = token.id_token.as_mut() {
        oidc_client.decode_token(&mut id_token)?;
        oidc_client.validate_token(&id_token, None, None)?;
        info!("token: {:#?}", id_token);
    } else {
        return Ok(None);
    }

    let userinfo = oidc_client.request_userinfo(&token).await?;

    info!("user info: {:#?}", userinfo);

    Ok(Some((token, userinfo)))
}

async fn reply_login(
    oidc_client: Arc<OpenIDClient>,
    host: String,
    login_query: LoginQuery,
) -> Result<impl warp::Reply, Infallible> {
    let request_token = request_token(oidc_client, &login_query).await;
    match request_token {
        Ok(Some((token, user_info))) => {
            let id = uuid::Uuid::new_v4().to_string();

            let login = user_info.preferred_username.clone();
            let email = user_info.email.clone();

            let user = User {
                id: user_info.sub.clone().unwrap_or_default(),
                login,
                first_name: user_info.name.clone(),
                email,
                activated: user_info.email_verified,
                permissions: Permissions::Moderator,
            };

            // TODO correct expiry
            let authorization_cookie = Cookie::build(COOKIE, &id)
                .path("/")
                .http_only(true)
                .secure(true)
                .finish()
                .to_string();

            SESSIONS
                .write()
                .await
                .map
                .insert(id, (user, token, user_info));

            let redirect_url = login_query.state.clone().unwrap_or(host);

            Ok(Response::builder()
                .status(StatusCode::FOUND)
                .header(warp::http::header::LOCATION, redirect_url)
                .header(warp::http::header::SET_COOKIE, authorization_cookie)
                .body("")
                .unwrap())
        }
        Ok(None) => {
            error!("login error in call: no id_token found");

            Ok(Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body("")
                .unwrap())
        }
        Err(err) => {
            error!("login error in call: {:#?}", err);

            Ok(Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body("")
                .unwrap())
        }
    }
}

async fn reply_logout(token: String) -> Result<impl warp::Reply, Infallible> {
    let deleted_cookie = Cookie::build(COOKIE, "")
        .path("/")
        .http_only(true)
        .secure(true)
        .expires(OffsetDateTime::now_utc())
        .finish()
        .to_string();

    SESSIONS.write().await.map.remove(&token);

    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(warp::http::header::LOCATION, "/search")
        .header(warp::http::header::SET_COOKIE, deleted_cookie)
        .body("")
        .unwrap())
}

async fn reply_authorize(
    oidc_client: Arc<OpenIDClient>,
    origin_url: String,
) -> Result<impl warp::Reply, Infallible> {
    let auth_url = oidc_client.auth_url(&Options {
        scope: Some("openid email profile".into()),
        state: Some(origin_url),
        ..Default::default()
    });

    info!("authorize: {}", auth_url);

    let url = auth_url.to_string();

    Ok(warp::reply::with_header(
        StatusCode::FOUND,
        warp::http::header::LOCATION,
        url,
    ))
}

#[derive(Debug)]
pub enum Unauthorized {
    NotLoggedIn,
    NoPermissions,
}

impl reject::Reject for Unauthorized {}

async fn extract_user(session_id: Option<String>) -> Result<User, Rejection> {
    if let Some(session_id) = session_id {
        if let Some((user, _, _)) = SESSIONS.read().await.map.get(&session_id) {
            Ok(user.clone())
        } else {
            Err(warp::reject::custom(Unauthorized::NotLoggedIn))
        }
    } else {
        Err(warp::reject::custom(Unauthorized::NotLoggedIn))
    }
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
    warp::cookie::optional(COOKIE)
        .and_then(extract_user)
        .map(|user| Auth { user: Some(user) })
        .or(warp::any().map(Auth::default))
        .unify()
        .and(warp::any().map(move || DbImpl(db.0.clone())))
}

pub fn with_user_auth(
    db: DbBase,
) -> impl Filter<Extract = (User, impl UserAccessDb), Error = Rejection> + Clone {
    warp::cookie::optional(COOKIE)
        .and_then(extract_user)
        .and(warp::any().map(move || DbImpl(db.0.clone())))
}

pub fn with_moderator_auth(
    db: DbBase,
) -> impl Filter<Extract = (User, impl ModeratorAccessDb), Error = Rejection> + Clone {
    warp::cookie::optional(COOKIE)
        .and_then(|token| async {
            match extract_user(token).await {
                Ok(user) if user.permissions.contains(Permissions::Moderator) => Ok(user),
                Ok(unauthorized) => {
                    Err(warp::reject::custom(Unauthorized::NoPermissions))
                }
                Err(e) => Err(e),
            }
        })
        .and(warp::any().map(move || DbImpl(db.0.clone())))
}
