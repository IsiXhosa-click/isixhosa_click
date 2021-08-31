use crate::auth::{Permissions, PublicAccessDb, User, ModeratorAccessDb, random_string_token, StaySignedInToken, UserAccessDb};
use openid::{Token, Userinfo};
use r2d2_sqlite::rusqlite::Row;
use rusqlite::{params, OptionalExtension};
use std::convert::TryFrom;
use argon2::{Argon2, PasswordVerifier, PasswordHash, PasswordHasher};
use std::time::Duration;
use chrono::Utc;

impl TryFrom<&Row<'_>> for User {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(User {
            id: row.get::<&str, i64>("user_id")? as u64,
            username: row.get("username")?,
            display_name: row.get("display_name")?,
            advanced_submit_form: row.get("advanced_submit_form")?,
            email: row.get("email")?,
            permissions: if row.get("is_moderator")? {
                Permissions::Moderator
            } else {
                Permissions::User
            },
        })
    }
}

impl User {
    pub fn fetch_by_id(
        db: &impl PublicAccessDb,
        id: u64,
    ) -> Option<User> {
        const SELECT: &str = "
            SELECT
                user_id, username, display_name, email, is_moderator, advanced_submit_form, locked
            FROM users
            WHERE user_id = ?1;
        ";

        let conn = db.get().unwrap();

        #[allow(clippy::redundant_closure)] // lifetime issue
            let user = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![id], |row| User::try_from(row))
            .optional()
            .unwrap();
        user
    }

    pub fn fetch_by_oidc_id(
        db: &impl PublicAccessDb,
        _proof: Token, // Make sure this is not called from the wrong context
        oidc_id: String,
    ) -> Option<User> {
        const SELECT: &str = "
            SELECT
                user_id, username, display_name, email, is_moderator, advanced_submit_form, locked
            FROM users
            WHERE oidc_id = ?1;
        ";

        let conn = db.get().unwrap();

        #[allow(clippy::redundant_closure)] // lifetime issue
        let user = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![oidc_id], |row| User::try_from(row))
            .optional()
            .unwrap();
        user
    }

    pub fn register(
        db: &impl PublicAccessDb,
        userinfo: Userinfo,
        username: String,
        display_name: bool,
        advanced_submit_form: bool,
        email: String,
        permissions: Permissions,
    ) -> User {
        const INSERT: &str = "
            INSERT INTO users
                (oidc_id, username, display_name, email, is_moderator, advanced_submit_form, locked)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) RETURNING user_id;
        ";

        let conn = db.get().unwrap();
        let mut stmt = conn.prepare(INSERT).unwrap();
        let params = params![
            userinfo.sub.unwrap(),
            username,
            display_name,
            email,
            permissions.contains(Permissions::Moderator), // is_moderator
            advanced_submit_form,
            false, // locked
        ];

        let id: i64 = stmt.query_row(params, |row| row.get("user_id")).unwrap();

        User {
            id: id as u64,
            username,
            display_name,
            advanced_submit_form,
            email,
            permissions,
        }
    }
}

impl StaySignedInToken {
    pub fn new(db: &impl PublicAccessDb, user_id: u64) -> (String, u64) {
        const INSERT: &str = "
            INSERT INTO login_tokens (token_hash, user_id, last_used)
            VALUES (?1, ?2, ?3)
            RETURNING token_id;
        ";

        let argon2 = Argon2::default();
        let token = random_string_token();
        let salt = random_string_token();

        let token_hash = argon2.hash_password(token.as_bytes(), &salt).unwrap();

        let conn = db.get().unwrap();
        let token_id: i64 = conn.prepare(INSERT)
            .unwrap()
            .query_row(params![token_hash.to_string(), user_id, Utc::now()], |row| row.get("token_id"))
            .unwrap();

        (token, token_id as u64)
    }

    pub fn delete(self, db: &impl UserAccessDb) {
        const DELETE: &str = "DELETE FROM login_tokens WHERE token_id = ?1;";

        let conn = db.get().unwrap();
        conn.prepare(DELETE).unwrap().execute(params![self.token_id]).unwrap();
    }

    /// Verifies the hash, returning the user id if successful
    pub fn verify_token(
        &self,
        db: &impl PublicAccessDb,
    ) -> Option<u64> {
        const SELECT: &str =
            "SELECT token_hash, user_id FROM login_tokens WHERE token_id = ?1;";
        const UPDATE: &str =
            "UPDATE login_tokens SET last_used = ?1 WHERE token_id = ?2;";

        let conn = db.get().unwrap();
        let (token_hash, user_id): (String, i64) = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![self.token_id], |row| Ok((row.get("token_hash")?, row.get("user_id")?)))
            .optional()
            .unwrap()?;

        let password_hash = &PasswordHash::new(&token_hash).ok()?;
        let argon2 = Argon2::default();

        argon2.verify_password(self.token.as_bytes(), password_hash)
            .ok()
            .map(|_| {
                conn.prepare(UPDATE)
                    .unwrap()
                    .execute(params![self.token_id, Utc::now()])
                    .unwrap();
                user_id as u64
            })
    }
}


pub async fn sweep_tokens(db: impl ModeratorAccessDb) {
    const DELETE: &str =
        "DELETE FROM login_tokens DATE_PART('days', NOW()::timestamp - last_used) > ?1;";
    const TOKEN_EXPIRY_DAYS: f64 = 14.0;
    const ONE_DAY: Duration = Duration::from_secs(60 * 60 * 24);

    loop {
        tokio::time::sleep(ONE_DAY).await;
        let conn = db.get().unwrap();
        conn.prepare(DELETE).unwrap().execute(params![TOKEN_EXPIRY_DAYS]).unwrap();
    }
}