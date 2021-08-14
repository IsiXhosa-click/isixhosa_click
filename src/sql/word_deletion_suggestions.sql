CREATE TABLE IF NOT EXISTS word_deletion_suggestions (
   suggestion_id  INTEGER PRIMARY KEY AUTOINCREMENT, -- id must be stable with deletion
   word_id        INTEGER NOT NULL UNIQUE ON CONFLICT IGNORE REFERENCES words(word_id) ON DELETE CASCADE
);
