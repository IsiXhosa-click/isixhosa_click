CREATE TABLE IF NOT EXISTS example_deletion_suggestions (
   suggestion_id  INTEGER PRIMARY KEY,
   example_id     INTEGER NOT NULL UNIQUE ON CONFLICT IGNORE REFERENCES examples(example_id) ON DELETE CASCADE
);
