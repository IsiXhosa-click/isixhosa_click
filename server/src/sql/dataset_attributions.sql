CREATE TABLE IF NOT EXISTS dataset_attributions (
    word_id     INTEGER NOT NULL REFERENCES words(word_id) ON DELETE CASCADE,
    dataset_id  INTEGER NOT NULL REFERENCES datasets(dataset_id) ON DELETE CASCADE,
    UNIQUE(word_id, dataset_id) ON CONFLICT IGNORE
);
