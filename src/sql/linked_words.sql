CREATE TABLE IF NOT EXISTS linked_words (
    link_id         INTEGER PRIMARY KEY,
    link_type       INTEGER NOT NULL,
    first_word_id   INTEGER NOT NULL REFERENCES words(word_id) ON DELETE CASCADE,
    second_word_id  INTEGER NOT NULL REFERENCES words(word_id) ON DELETE CASCADE
);
