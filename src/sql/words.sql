CREATE TABLE IF NOT EXISTS words (
    word_id              INTEGER PRIMARY KEY,
    english              TEXT NOT NULL,
    xhosa                TEXT NOT NULL,
    part_of_speech       INTEGER NOT NULL,

    xhosa_tone_markings  TEXT,
    infinitive           TEXT,
    is_plural            BOOLEAN,
    noun_class           INTEGER,
    example_english      TEXT,
    example_xhosa        TEXT,
    note                 TEXT
);
