CREATE TABLE IF NOT EXISTS example_suggestions (
    suggestion_id            INTEGER PRIMARY KEY,
    link_type                INTEGER NOT NULL,
    first_existing_word_id   INTEGER REFERENCES words(word_id) ON DELETE CASCADE,
    second_existing_word_id  INTEGER REFERENCES words(word_id) ON DELETE CASCADE,
    suggested_word_id        INTEGER REFERENCES words(word_id) ON DELETE CASCADE
);
