CREATE TABLE IF NOT EXISTS user_attributions (
    word_id  INTEGER NOT NULL REFERENCES words(word_id) ON DELETE CASCADE,
    user_id  INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE(word_id, user_id) ON CONFLICT IGNORE
);
