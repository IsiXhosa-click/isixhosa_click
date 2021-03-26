CREATE TABLE IF NOT EXISTS example_suggestions (
    suggestion_id      INTEGER PRIMARY KEY,
    -- In the case of adding a new example to an existing word
    existing_word_id   INTEGER REFERENCES words(word_id) ON DELETE CASCADE,
    -- In the case of adding a new example to a suggested word
    suggested_word_id  INTEGER REFERENCES words(word_id) ON DELETE CASCADE,
    example_english    TEXT NOT NULL,
    example_xhosa      TEXT NOT NULL
);
