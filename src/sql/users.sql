CREATE TABLE IF NOT EXISTS users (
    user_id              INTEGER PRIMARY KEY,
    username             TEXT NOT NULL,
    email                TEXT NOT NULL,
    password_hash        TEXT NOT NULL,
    hash_scheme_version  INTEGER NOT NULL,
    permissions_level    INTEGER NOT NULL,
    email_verified       BOOLEAN NOT NULL,
    compromised          BOOLEAN NOT NULL,
    locked               BOOLEAN NOT NULL,
    banned               BOOLEAN NOT NULL
);
