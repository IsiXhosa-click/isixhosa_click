use crate::database::WordOrSuggestedId;
use crate::language::{NounClass, PartOfSpeech, WordLinkType};
use rusqlite::{params, Row};
use std::convert::{TryFrom, TryInto};
use r2d2_sqlite::SqliteConnectionManager;
use r2d2::Pool;
use crate::database::suggestion::{SuggestedWord, SuggestedLinkedWord, SuggestedExample};

pub struct ExistingWord {
    pub word_id: i64,

    pub english: String,
    pub xhosa: String,
    pub part_of_speech: PartOfSpeech,

    pub xhosa_tone_markings: String,
    pub infinitive: String,
    pub is_plural: bool,
    pub noun_class: Option<NounClass>,
    pub note: String,

    pub examples: Vec<ExistingExample>,
    pub linked_words: Vec<ExistingLinkedWord>,
}

impl TryFrom<&Row<'_>> for ExistingWord {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, rusqlite::Error> {
        Ok(ExistingWord {
            word_id: row.get("word_id")?,
            english: row.get("english")?,
            xhosa: row.get("xhosa")?,
            part_of_speech: row.get("part_of_speech")?,
            xhosa_tone_markings: row.get("xhosa_tone_markings")?,
            infinitive: row.get("infinitive")?,
            is_plural: row.get("is_plural")?,
            noun_class: row.get("noun_class")?,
            note: row.get("note")?,
            examples: vec![],
            linked_words: vec![],
        })
    }
}

pub struct ExistingExample {
    pub example_id: i64,
    pub word_or_suggested_id: WordOrSuggestedId,

    pub english: String,
    pub xhosa: String,
}

impl TryFrom<&Row<'_>> for ExistingExample {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(ExistingExample {
            example_id: row.get("example_id")?,
            english: row.get("english")?,
            xhosa: row.get("xhosa")?,
            word_or_suggested_id: row.try_into()?,
        })
    }
}

pub struct ExistingLinkedWord {
    pub link_id: i64,
    pub first_word_id: i64,
    pub second_word_id: i64,
    pub link_type: WordLinkType,
}

impl TryFrom<&Row<'_>> for ExistingLinkedWord {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(ExistingLinkedWord {
            link_id: row.get("link_id")?,
            first_word_id: row.get("first_word_id")?,
            second_word_id: row.get("second_word_id")?,
            link_type: row.get("link_type")?,
        })
    }
}

pub fn accept_new_word_suggestion(db: Pool<SqliteConnectionManager>, s: SuggestedWord) -> i64 {
    let word_id = accept_word_suggestion(db.clone(), &s);

    for mut example in s.examples.into_iter() {
        example.word_or_suggested_id = WordOrSuggestedId::ExistingWord(word_id);
        accept_example(db.clone(), example);
    }

    for mut linked_word in s.linked_words.into_iter() {
        linked_word.second = WordOrSuggestedId::ExistingWord(word_id);
        accept_linked_word(db.clone(), linked_word);
    }

    word_id
}

pub fn accept_word_suggestion(db: Pool<SqliteConnectionManager>, s: &SuggestedWord) -> i64 {
    const INSERT: &str = "
        INSERT INTO words (
            word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            noun_class, note
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(word_id) DO UPDATE SET
                english = excluded.english,
                xhosa = excluded.xhosa,
                part_of_speech = excluded.part_of_speech,
                xhosa_tone_markings = excluded.xhosa_tone_markings,
                infinitive = excluded.infinitive,
                is_plural = excluded.is_plural,
                noun_class = excluded.noun_class,
                note = excluded.note
            RETURNING word_id;
    ";
    const DELETE: &str = "DELETE FROM word_suggestions WHERE suggestion_id = ?1";

    let conn = db.get().unwrap();
    let params = params![
        s.word_id, s.english.current(), s.xhosa.current(), s.part_of_speech.current(),
        s.xhosa_tone_markings.current(), s.infinitive.current(), s.is_plural.current(),
        s.noun_class.current(), s.note.current(),
    ];

    let id = conn
        .prepare(INSERT)
        .unwrap()
        .query_row(params, |row| row.get("word_id"))
        .unwrap();

    conn.prepare(DELETE).unwrap().execute(params![s.suggestion_id]).unwrap();

    id
}

pub fn accept_linked_word(
    db: Pool<SqliteConnectionManager>,
    s: SuggestedLinkedWord,
) -> i64 {
    const INSERT: &str = "
        INSERT INTO linked_words (link_id, link_type, first_word_id, second_word_id)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(link_id) DO UPDATE SET
                link_type = excluded.link_type
            RETURNING link_id;
    ";
    const DELETE: &str = "DELETE FROM linked_word_suggestions WHERE suggestion_id = ?1";

    let conn = db.get().unwrap();
    let second_existing = match s.second {
        WordOrSuggestedId::ExistingWord(e) => e,
        _ => panic!("No existing word for suggested linked word {:#?}", s),
    };

    let params = params![
        s.existing_linked_word_id, s.link_type.current(), s.first_existing_word_id, second_existing
    ];

    let id = conn
        .prepare(INSERT)
        .unwrap()
        .query_row(params, |row| row.get("link_id"))
        .unwrap();

    conn.prepare(DELETE).unwrap().execute(params![s.suggestion_id]).unwrap();

    id
}

pub fn accept_example(db: Pool<SqliteConnectionManager>, s: SuggestedExample) -> i64 {
    const INSERT: &str = "
        INSERT INTO examples (example_id, word_id, english, xhosa) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(example_id) DO UPDATE SET
                english = excluded.english,
                xhosa = excluded.xhosa
            RETURNING example_id;
    ";
    const DELETE: &str = "DELETE FROM example_suggestions WHERE suggestion_id = ?1";

    let conn = db.get().unwrap();
    let word = match s.word_or_suggested_id {
        WordOrSuggestedId::ExistingWord(e) => e,
        _ => panic!("No existing word for suggested example {:#?}", s),
    };
    let params = params![
        s.existing_example_id, word, s.english.current(), s.xhosa.current()
    ];

    let id = conn
        .prepare(INSERT)
        .unwrap()
        .query_row(params, |row| row.get("example_id"))
        .unwrap();

    conn.prepare(DELETE).unwrap().execute(params![s.suggestion_id]).unwrap();

    id
}
