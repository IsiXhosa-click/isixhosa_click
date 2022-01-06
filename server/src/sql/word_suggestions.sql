CREATE TABLE IF NOT EXISTS word_suggestions (
    suggestion_id        INTEGER PRIMARY KEY AUTOINCREMENT,
    suggesting_user      INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    -- In case of an update to an existing word
    existing_word_id     INTEGER REFERENCES words(word_id) ON DELETE CASCADE,
    changes_summary      TEXT NOT NULL,

    english              TEXT,
    xhosa                TEXT,
    part_of_speech       INTEGER,

    xhosa_tone_markings  TEXT,
    infinitive           TEXT,
    is_plural            BOOLEAN,
    is_inchoative        BOOLEAN,
    is_informal          BOOLEAN,
    transitivity         INTEGER,
    followed_by          TEXT,
    -- 255 is sentinel for "no noun class" as opposed to null which is noun class not changed
    noun_class           INTEGER,
    note                 TEXT
);
