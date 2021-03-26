CREATE TABLE IF NOT EXISTS word_suggestions (
    suggestion_id        INTEGER PRIMARY KEY,
    -- In case of an update to an existing word
    existing_word_id     INTEGER REFERENCES words(word_id) ON DELETE CASCADE,
    changes_summary      TEXT NOT NULL,
    deletion             BOOLEAN NOT NULL,

    english              TEXT,
    xhosa                TEXT,
    part_of_speech       INTEGER,

    xhosa_tone_markings  TEXT,
    infinitive           TEXT,
    is_plural            BOOLEAN,
    noun_class           INTEGER,
    example_english      TEXT,
    example_xhosa        TEXT,
    note                 TEXT
);
