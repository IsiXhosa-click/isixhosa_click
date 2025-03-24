use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use isixhosa_common::language::WordLinkType;
use crate::{set_up_db, Config};

pub mod zulu_lsp;
pub mod isindebele_birds;

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

pub fn insert_word(conn: &Connection, english: &str, target_lang_word: &str) -> i64 {
    const INSERT: &str = "INSERT INTO words
        (english, xhosa, xhosa_tone_markings, infinitive, is_plural, is_inchoative, is_informal, followed_by, note)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);";

    let mut insert = conn.prepare(INSERT).unwrap();

    insert
        .execute(params![english, target_lang_word, "", "", false, false, false, "", ""])
        .unwrap();
    conn.last_insert_rowid()
}
