CREATE TABLE IF NOT EXISTS words (
    word_id              INTEGER PRIMARY KEY AUTOINCREMENT,
    english              TEXT NOT NULL,
    xhosa                TEXT NOT NULL,
    part_of_speech       INTEGER NOT NULL,

    xhosa_tone_markings  TEXT NOT NULL,
    infinitive           TEXT NOT NULL,
    is_plural            BOOLEAN NOT NULL,
    is_inchoative        BOOLEAN NOT NULL,
    is_informal          BOOLEAN NOT NULL,
    transitivity         INTEGER,
    followed_by          TEXT NOT NULL,
    noun_class           INTEGER,
    note                 TEXT NOT NULL
);
