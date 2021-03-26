CREATE TABLE IF NOT EXISTS verification_emails (
    token            TEXT NOT NULL,
    user_id          INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    expiration_date  TIMESTAMP WITH TIME ZONE NOT NULL
);
