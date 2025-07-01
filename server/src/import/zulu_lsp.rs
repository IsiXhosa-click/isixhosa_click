use crate::import::{force_reindex, get_db_for_import, insert_word, link_words_alt_use};
use crate::Config;
use anyhow::{Context, Result};
use itertools::Itertools;
use rusqlite::Connection;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub fn import_zulu_lsp(cfg: Config, path: &Path) -> Result<()> {
    let conn = get_db_for_import(&cfg)?;
    let file = BufReader::new(File::open(path)?);

    for line in file.lines() {
        let line = line?;
        if let Err(err) = process_line(&conn, line.clone()) {
            eprintln!("Skipping line {line} due to error {err:?}");
        }
    }

    force_reindex(&cfg)
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

/// Extract all the terms contained in the word and strip them of whitespace
fn process_word(word: &str) -> impl Iterator<Item = &str> + Clone {
    word.split(&[',', ';']).map(|word| word.trim())
}
