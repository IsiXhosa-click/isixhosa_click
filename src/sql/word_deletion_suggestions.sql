CREATE TABLE IF NOT EXISTS word_deletion_suggestions (
   suggestion_id    INTEGER PRIMARY KEY AUTOINCREMENT, -- id must be stable with deletion
   suggesting_user  INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
   word_id          INTEGER NOT NULL UNIQUE ON CONFLICT IGNORE REFERENCES words(word_id) ON DELETE CASCADE,
   reason           TEXT NOT NULL
);
