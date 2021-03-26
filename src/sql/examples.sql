CREATE TABLE IF NOT EXISTS examples (
    example_id           INTEGER PRIMARY KEY,
    word_id              INTEGER NOT NULL REFERENCES words(word_id) ON DELETE CASCADE,
    example_english      TEXT NOT NULL,
    example_xhosa        TEXT NOT NULL
);
