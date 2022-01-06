CREATE TABLE IF NOT EXISTS users (
    user_id               INTEGER PRIMARY KEY AUTOINCREMENT,
    oidc_id               TEXT NOT NULL UNIQUE,
    username              TEXT NOT NULL,
    display_name          BOOLEAN NOT NULL,
    email                 TEXT NOT NULL,
    is_moderator          BOOLEAN NOT NULL,
    is_administrator      BOOLEAN NOT NULL,
    locked                BOOLEAN NOT NULL
);
