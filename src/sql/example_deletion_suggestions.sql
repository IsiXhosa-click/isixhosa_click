CREATE TABLE IF NOT EXISTS example_deletion_suggestions (
   suggestion_id    INTEGER PRIMARY KEY,
   suggesting_user  INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
   example_id       INTEGER NOT NULL UNIQUE ON CONFLICT IGNORE REFERENCES examples(example_id) ON DELETE CASCADE,
   reason           TEXT NOT NULL
);
