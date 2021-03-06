CREATE TABLE IF NOT EXISTS linked_word_deletion_suggestions (
   suggestion_id    INTEGER PRIMARY KEY AUTOINCREMENT,
   suggesting_user  INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
   linked_word_id   INTEGER NOT NULL UNIQUE ON CONFLICT IGNORE REFERENCES linked_words(link_id) ON DELETE CASCADE,
   reason           TEXT NOT NULL
);
