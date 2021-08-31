CREATE TABLE IF NOT EXISTS login_tokens (
    token_id    INTEGER PRIMARY KEY,
    token_hash  TEXT NOT NULL,
    user_id     INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    last_used   TIMESTAMP WITH TIME ZONE NOT NULL
);
