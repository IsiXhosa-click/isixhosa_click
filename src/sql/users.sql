CREATE TABLE IF NOT EXISTS users (
    user_id               INTEGER PRIMARY KEY AUTOINCREMENT,
    display_name          TEXT NOT NULL,
    email                 TEXT NOT NULL,
    is_moderator          BOOLEAN NOT NULL,
    advanced_submit_form  BOOLEAN NOT NULL,
    locked                BOOLEAN NOT NULL
);
