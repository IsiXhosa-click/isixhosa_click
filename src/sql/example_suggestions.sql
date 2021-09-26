CREATE TABLE IF NOT EXISTS example_suggestions (
    suggestion_id        INTEGER PRIMARY KEY AUTOINCREMENT, -- id must be stable with deletion
    suggesting_user      INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    -- In the case of adding a new example to an existing word
    existing_word_id     INTEGER REFERENCES words(word_id) ON DELETE CASCADE,
    -- In the case of adding a new example to a suggested word
    suggested_word_id    INTEGER REFERENCES word_suggestions(suggestion_id) ON DELETE CASCADE,
    -- In the case of updating an existing example
    existing_example_id  INTEGER REFERENCES examples(example_id) ON DELETE CASCADE,
    changes_summary      TEXT NOT NULL,
    english              TEXT,
    xhosa                TEXT
);
