CREATE TABLE IF NOT EXISTS examples (
    example_id  INTEGER PRIMARY KEY AUTOINCREMENT,
    word_id     INTEGER NOT NULL REFERENCES words(word_id) ON DELETE CASCADE,
    english     TEXT NOT NULL,
    xhosa       TEXT NOT NULL
);
