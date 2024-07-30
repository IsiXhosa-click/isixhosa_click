CREATE TABLE IF NOT EXISTS datasets (
    dataset_id   INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT NOT NULL,
    author       TEXT NOT NULL,
    license      TEXT NOT NULL,
    institution  TEXT,
    icon         BLOB,
    url          TEXT
);
