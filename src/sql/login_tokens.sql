CREATE TABLE IF NOT EXISTS login_tokens (
    token_hash           TEXT NOT NULL,
    hash_scheme_version  INTEGER NOT NULL,
    user_id              INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    last_used            TIMESTAMP WITH TIME ZONE NOT NULL,
    expiration_date      TIMESTAMP WITH TIME ZONE
);
