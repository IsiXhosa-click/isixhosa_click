use crate::{set_up_db, Config};
use anyhow::{Context, Result};
use isixhosa::noun::NounClass;
use isixhosa_common::language::{PartOfSpeech, Transitivity, WordLinkType};
use rusqlite::{params, Connection};

pub mod isindebele_birds;
pub(crate) mod medical_glossary;
pub mod zulu_lsp;

pub fn get_db_for_import(cfg: &Config) -> Result<Connection> {
    let conn = Connection::open(&cfg.database_path)?;
    set_up_db(&conn)?;
    Ok(conn)
}

/// Force reindex on next start
pub fn force_reindex(cfg: &Config) -> Result<()> {
    std::fs::remove_dir_all(&cfg.tantivy_path).context("Couldn't delete tantivy data directory")?;
    std::fs::create_dir_all(&cfg.tantivy_path).context("Couldn't create tantivy data directory")?;
    Ok(())
}

pub fn link_words_alt_use(conn: &Connection, one: i64, two: i64) {
    const INSERT: &str = "INSERT INTO linked_words
            (link_type, first_word_id, second_word_id)
        VALUES (?1, ?2, ?3)";

    let mut insert = conn.prepare(INSERT).unwrap();
    insert
        .execute(params![WordLinkType::AlternateUse as u8, one, two])
        .unwrap();
}

pub fn link_word_suggestions_alt_use(conn: &Connection, user: i64, one: i64, two: i64) {
    const INSERT: &str = "
        INSERT INTO linked_word_suggestions (
            suggesting_user, link_type, suggested_word_id, second_suggested_word_id, changes_summary
        ) VALUES (?1, ?2, ?3, ?4, ?5)
    ";

    let mut insert = conn.prepare(INSERT).unwrap();
    insert
        .execute(params![
            user,
            WordLinkType::AlternateUse as u8,
            one,
            two,
            ""
        ])
        .unwrap();
}

pub fn insert_word(conn: &Connection, english: &str, target_lang_word: &str) -> i64 {
    const INSERT: &str = "INSERT INTO words
        (english, xhosa, xhosa_tone_markings, infinitive, is_plural, is_inchoative, is_informal, followed_by, note)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);";

    let mut insert = conn.prepare(INSERT).unwrap();

    insert
        .execute(params![
            english,
            target_lang_word,
            "",
            "",
            false,
            false,
            false,
            "",
            ""
        ])
        .unwrap();
    conn.last_insert_rowid()
}

#[allow(clippy::too_many_arguments)] // It's a once-off function not worth defining a struct for
pub fn insert_suggested_word_with_info(
    conn: &Connection,
    english: &str,
    target_lang_word: &str,
    infinitive: &str,
    is_plural: bool,
    is_inchoative: bool,
    part_of_speech: Option<PartOfSpeech>,
    noun_class: Option<NounClass>,
    note: &str,
    transitivity: Option<Transitivity>,
) -> i64 {
    const INSERT: &str = "INSERT INTO word_suggestions
        (english, xhosa, xhosa_tone_markings, infinitive, is_plural, is_inchoative, is_informal,
        followed_by, note, part_of_speech, noun_class, transitivity, changes_summary, suggesting_user)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14);";

    let mut insert = conn.prepare(INSERT).unwrap();

    insert
        .execute(params![
            english,
            target_lang_word,
            "",
            infinitive,
            is_plural,
            is_inchoative,
            false,
            "",
            note,
            part_of_speech,
            noun_class.map(|v| v as i64),
            transitivity,
            "",
            1, // Just use user #1
        ])
        .unwrap();
    conn.last_insert_rowid()
}

pub fn insert_dataset_attribution_suggestions(
    conn: &Connection,
    suggestion: i64,
    dataset: i64,
) -> Result<()> {
    const INSERT_SUGGESTION: &str = "
        INSERT INTO dataset_attribution_suggestions
            (dataset_id, suggesting_user, changes_summary, existing_word_id, suggested_word_id, is_delete)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6);
    ";

    let mut insert = conn.prepare(INSERT_SUGGESTION)?;

    insert.execute(params![
        dataset,
        1, // Just use user #1
        "",
        None::<u64>,
        suggestion,
        false // This is _not_ a deletion
    ])?;

    Ok(())
}
