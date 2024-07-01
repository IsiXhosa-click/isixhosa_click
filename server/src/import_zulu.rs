use crate::{set_up_db, Config};
use anyhow::{Context, Result};
use isixhosa_common::language::WordLinkType;
use itertools::Itertools;
use rusqlite::{params, Connection};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub fn import_zulu_lsp(cfg: Config, path: &Path) -> Result<()> {
    let conn = Connection::open(&cfg.database_path)?;

    set_up_db(&conn)?;

    let file = BufReader::new(File::open(path)?);

    for line in file.lines() {
        let line = line?;
        if let Err(err) = process_line(&conn, line.clone()) {
            eprintln!("Skipping line {line} due to error {err:?}");
        }
    }

    // Force reindex on next start
    std::fs::remove_dir_all(&cfg.tantivy_path).context("Couldn't delete tantivy data directory")?;
    std::fs::create_dir_all(&cfg.tantivy_path).context("Couldn't create tantivy data directory")?;

    Ok(())
}

/// Extract all the terms contained in the line
fn process_line(conn: &Connection, line: String) -> Result<()> {
    let (english, zulu) = line.split_once(':').context("No translation found")?;

    let word_ids: Vec<i64> = process_word(english)
        .cartesian_product(process_word(zulu))
        .map(|(english, zulu)| insert_word(conn, english, zulu))
        .collect();

    word_ids
        .iter()
        .combinations(2)
        .for_each(|list| link_words_alt_use(conn, *list[0], *list[1]));

    Ok(())
}

fn link_words_alt_use(conn: &Connection, one: i64, two: i64) {
    const INSERT: &str = "INSERT INTO linked_words
            (link_type, first_word_id, second_word_id)
        VALUES (?1, ?2, ?3)";

    let mut insert = conn.prepare(INSERT).unwrap();
    insert
        .execute(params![WordLinkType::AlternateUse as u8, one, two])
        .unwrap();
}

fn insert_word(conn: &Connection, english: &str, zulu: &str) -> i64 {
    const INSERT: &str = "INSERT INTO words
        (english, xhosa, xhosa_tone_markings, infinitive, is_plural, is_inchoative, is_informal, followed_by, note)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);";

    let mut insert = conn.prepare(INSERT).unwrap();

    insert
        .execute(params![english, zulu, "", "", false, false, false, "", ""])
        .unwrap();
    conn.last_insert_rowid()
}

/// Extract all the terms contained in the word and strip them of whitespace
fn process_word(word: &str) -> impl Iterator<Item = &str> + Clone {
    word.split(&[',', ';']).map(|word| word.trim())
}
