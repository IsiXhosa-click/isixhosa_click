use futures::Future;
use futures::FutureExt;
use rand::RngCore;
use unicode_normalization::UnicodeNormalization;

pub const MAX_TOKEN_LENGTH: usize = 45;
pub const MAX_PASSWORD_LEN: usize = 512;
pub const MIN_PASSWORD_LEN: usize = 12;
pub const MAX_USERNAME_LEN: usize = 64;
pub const MIN_USERNAME_LEN: usize = 3;

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum AuthError {
    PasswordTooShort,
    PasswordTooLong,
    UsernameTooShort,
    UsernameTooLong,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
#[repr(u8)]
pub enum HashSchemeVersion {
    Argon2V1 = 1,
}

impl HashSchemeVersion {
    pub const LATEST: HashSchemeVersion = HashSchemeVersion::Argon2V1;
}

impl From<i16> for HashSchemeVersion {
    fn from(v: i16) -> Self {
        match v {
            1 => HashSchemeVersion::Argon2V1,
            invalid_version => panic!("Invalid hash scheme version {}", invalid_version),
        }
    }
}

fn valid_password(password: &str) -> bool {
    password.len() <= MAX_PASSWORD_LEN && password.len() >= MIN_PASSWORD_LEN
}

fn valid_username(username: &str) -> bool {
    username.len() <= MAX_USERNAME_LEN && username.len() >= MIN_USERNAME_LEN
}

pub struct TooShort;

fn normalize_username(username: &str) -> String {
    username.nfkc().flat_map(|c| c.to_lowercase()).collect()
}

fn prepare_username(username: &str) -> Result<String, TooShort> {
    if valid_username(username) {
        Ok(normalize_username(username))
    } else {
        Err(TooShort)
    }
}

fn hash(pass: String) -> impl Future<Output = (String, HashSchemeVersion)> {
    tokio::task::spawn_blocking(move || {
        let mut salt: [u8; 32] = [0; 32]; // 256 bits
        rand::thread_rng().fill_bytes(&mut salt);
        let config = Default::default();

        let hash = argon2::hash_encoded(pass.as_bytes(), &salt, &config)
            .expect("Error generating password hash");

        (hash, HashSchemeVersion::Argon2V1)
    })
    .map(|r| r.expect("Error in tokio password hashing task"))
}

fn verify_credentials(
    pass: String,
    hash: String,
    scheme_version: HashSchemeVersion,
) -> impl Future<Output = bool> {
    tokio::task::spawn_blocking(move || {
        use HashSchemeVersion::*;

        match scheme_version {
            Argon2V1 => argon2::verify_encoded(&hash, pass.as_bytes())
                .expect("Error verifying password hash"),
        }
    })
    .map(|r| r.expect("Error in tokio password verifying task"))
}

// pub async fn verify_user(username: String, db: &Pool<SqliteConnectionManager>, password: String) -> bool {
//     let conn = db.get().unwrap();
//     conn.prepare("SELECT password, "); // TODO
//     verify_credentials(password, user.password_hash, user.hash_scheme_version).await
// }
