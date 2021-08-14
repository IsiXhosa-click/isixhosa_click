CREATE TABLE IF NOT EXISTS linked_word_deletion_suggestions (
   suggestion_id   INTEGER PRIMARY KEY AUTOINCREMENT,
   linked_word_id  INTEGER NOT NULL UNIQUE ON CONFLICT IGNORE REFERENCES linked_words(link_id) ON DELETE CASCADE
);
