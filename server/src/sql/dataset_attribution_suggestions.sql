CREATE TABLE IF NOT EXISTS dataset_attribution_suggestions (
    suggestion_id        INTEGER PRIMARY KEY AUTOINCREMENT, -- id must be stable with deletion
    dataset_id           INTEGER NOT NULL REFERENCES datasets(dataset_id) ON DELETE CASCADE,
    suggesting_user      INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    changes_summary      TEXT NOT NULL,
    is_delete            BOOLEAN NOT NULL, -- whether or not this suggestion is to delete a given existing attribution

    -- In the case of adding an attribution to an existing word
    existing_word_id     INTEGER REFERENCES words(word_id) ON DELETE CASCADE,
    -- In the case of adding an attribution to a suggested word
    suggested_word_id    INTEGER REFERENCES word_suggestions(suggestion_id) ON DELETE CASCADE
);
