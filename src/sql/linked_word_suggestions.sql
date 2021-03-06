CREATE TABLE IF NOT EXISTS linked_word_suggestions (
    suggestion_id             INTEGER PRIMARY KEY AUTOINCREMENT,
    link_type                 INTEGER,
    suggesting_user           INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    changes_summary           TEXT NOT NULL,
    existing_linked_word_id   INTEGER REFERENCES linked_words(link_id) ON DELETE CASCADE,
    first_existing_word_id    INTEGER REFERENCES words(word_id) ON DELETE CASCADE,
    second_existing_word_id   INTEGER REFERENCES words(word_id) ON DELETE CASCADE,
    suggested_word_id         INTEGER REFERENCES word_suggestions(suggestion_id) ON DELETE CASCADE,
    second_suggested_word_id  INTEGER REFERENCES word_suggestions(suggestion_id) ON DELETE CASCADE
);
