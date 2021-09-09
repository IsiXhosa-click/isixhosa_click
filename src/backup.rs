//! This script is called daily to back up the database and sweep unused login tokens.

use crate::database::existing::{ExistingExample, ExistingWord};
use crate::language::{PartOfSpeech, WordLinkType};
use crate::serialization::{deser_from_str, ser_to_debug};
use crate::{Config, set_up_db};
use fallible_iterator::FallibleIterator;
use isixhosa::noun::NounClass;
use rusqlite::backup::Backup;
use rusqlite::params;
use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fs::File;
use std::io::{BufWriter, BufReader, Write};
use std::time::Duration;
use tempdir::TempDir;
use std::process::Command;
use std::io;
use chrono::Utc;

pub fn restore(cfg: Config) {
    let conn = Connection::open(&cfg.database_path).unwrap();

    set_up_db(&conn);
    restore_words(&cfg, &conn);
    restore_examples(&cfg, &conn);
    restore_linked_words(&cfg, &conn);

    // Force reindex on next start
    std::fs::remove_dir_all(&cfg.tantivy_path).unwrap();
    std::fs::create_dir_all(&cfg.tantivy_path).unwrap();
}

pub fn run_daily_tasks(cfg: Config) {
    let conn = Connection::open(&cfg.database_path).unwrap();
    sweep_tokens(&conn);
    backup(&cfg, &conn);
}

fn backup(cfg: &Config, src: &Connection) {
    let temp_dir = TempDir::new("isixhosa_click_backup").unwrap();
    let temp_db = temp_dir.path().join("isixhosa_click.bak.db");
    let mut dest = Connection::open(temp_db).unwrap();

    {
        let backup = Backup::new(src, &mut dest).unwrap();
        backup
            .run_to_completion(5, Duration::from_millis(250), None)
            .unwrap();
    }

    write_words(cfg, &dest);
    write_examples(cfg, &dest);
    write_linked_words(cfg, &dest);

    let output = Command::new("git")
        .current_dir(&cfg.plaintext_export_path)
        .args(["commit", "-a", "-m", &format!("Daily backup for {}", Utc::now().date())])
        .output()
        .unwrap();

    io::stdout().write_all(&output.stdout).unwrap();
    io::stderr().write_all(&output.stderr).unwrap();


    let output = Command::new("git")
        .current_dir(&cfg.plaintext_export_path)
        .arg("push")
        .output()
        .unwrap();

    io::stdout().write_all(&output.stdout).unwrap();
    io::stderr().write_all(&output.stderr).unwrap();
}

#[derive(Serialize, Deserialize)]
pub struct WordRecord {
    pub word_id: u64,

    pub english: String,
    pub xhosa: String,
    #[serde(serialize_with = "ser_to_debug")]
    #[serde(deserialize_with = "deser_from_str")]
    pub part_of_speech: PartOfSpeech,

    pub xhosa_tone_markings: String,
    pub infinitive: String,
    pub is_plural: bool,
    pub noun_class: Option<NounClass>,
    pub note: String,
}

impl From<ExistingWord> for WordRecord {
    fn from(w: ExistingWord) -> Self {
        WordRecord {
            word_id: w.word_id,
            english: w.english,
            xhosa: w.xhosa,
            part_of_speech: w.part_of_speech,
            xhosa_tone_markings: w.xhosa_tone_markings,
            infinitive: w.infinitive,
            is_plural: w.is_plural,
            noun_class: w.noun_class,
            note: w.note,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct LinkedWordRecord {
    pub link_id: u64,
    #[serde(serialize_with = "ser_to_debug")]
    #[serde(deserialize_with = "deser_from_str")]
    pub link_type: WordLinkType,
    pub first: u64,
    pub second: u64,
}

impl TryFrom<&Row<'_>> for LinkedWordRecord {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(LinkedWordRecord {
            link_id: row.get("link_id")?,
            link_type: row.get("link_type")?,
            first: row.get("first_word_id")?,
            second: row.get("second_word_id")?,
        })
    }
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn write_words(cfg: &Config, conn: &Connection) {
    const SELECT_ORIGINAL: &str = "
        SELECT
            word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            noun_class, note
        FROM words
        ORDER BY word_id;
    ";

    let path = cfg.plaintext_export_path.join("words.csv");
    let writer = BufWriter::new(File::create(path).unwrap());
    let mut csv = csv::Writer::from_writer(writer);

    conn.prepare(SELECT_ORIGINAL)
        .unwrap()
        .query(params![])
        .unwrap()
        .map(|row| Ok(WordRecord::from(ExistingWord::try_from(row)?)))
        .map_err(|e| -> anyhow::Error { e.into() })
        .for_each(|word| csv.serialize(word).map_err(Into::into))
        .unwrap();
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn restore_words(cfg: &Config, conn: &Connection) {
    const INSERT: &str = "
        INSERT INTO words (
            word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            noun_class, note
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);
    ";

    let path = cfg.plaintext_export_path.join("words.csv");
    let reader = BufReader::new(File::open(path).unwrap());
    let mut csv = csv::Reader::from_reader(reader);
    let mut insert = conn.prepare(INSERT).unwrap();

    for res in csv.deserialize() {
        let w: WordRecord = res.unwrap();

        insert.execute(params![
            w.word_id,
            w.english,
            w.xhosa,
            w.part_of_speech,
            w.xhosa_tone_markings,
            w.infinitive,
            w.is_plural,
            w.noun_class.map(|x| x as u8),
            w.note
        ]).unwrap();
    }
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn write_examples(cfg: &Config, conn: &Connection) {
    const SELECT: &str = "
        SELECT example_id, word_id, english, xhosa
        FROM examples
        ORDER BY example_id;
    ";

    let path = cfg.plaintext_export_path.join("examples.csv");
    let writer = BufWriter::new(File::create(path).unwrap());
    let mut csv = csv::Writer::from_writer(writer);

    conn.prepare(SELECT)
        .unwrap()
        .query(params![])
        .unwrap()
        .map(|row| ExistingExample::try_from(row))
        .map_err(|e| -> anyhow::Error { e.into() })
        .for_each(|example| csv.serialize(example).map_err(Into::into))
        .unwrap()
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn restore_examples(cfg: &Config, conn: &Connection) {
    const INSERT: &str = "
        INSERT INTO examples (example_id, word_id, english, xhosa) VALUES (?1, ?2, ?3, ?4);
    ";

    let path = cfg.plaintext_export_path.join("examples.csv");
    let reader = BufReader::new(File::open(path).unwrap());
    let mut csv = csv::Reader::from_reader(reader);
    let mut insert = conn.prepare(INSERT).unwrap();

    for res in csv.deserialize() {
        let e: ExistingExample = res.unwrap();
        insert.execute(params![e.example_id, e.word_id, e.english, e.xhosa]).unwrap();
    }
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn write_linked_words(cfg: &Config, conn: &Connection) {
    const SELECT: &str = "
        SELECT link_id, link_type, first_word_id, second_word_id
        FROM linked_words
        ORDER BY link_id;
    ";

    let path = cfg.plaintext_export_path.join("linked_words.csv");
    let writer = BufWriter::new(File::create(path).unwrap());
    let mut csv = csv::Writer::from_writer(writer);

    conn.prepare(SELECT)
        .unwrap()
        .query(params![])
        .unwrap()
        .map(|row| LinkedWordRecord::try_from(row))
        .map_err(|e| -> anyhow::Error { e.into() })
        .for_each(|example| csv.serialize(example).map_err(Into::into))
        .unwrap()
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn restore_linked_words(cfg: &Config, conn: &Connection) {
    const INSERT: &str = "
        INSERT INTO linked_words
            (link_id, link_type, first_word_id, second_word_id)
        VALUES (?1, ?2, ?3, ?4);
    ";

    let path = cfg.plaintext_export_path.join("linked_words.csv");
    let reader = BufReader::new(File::open(path).unwrap());
    let mut csv = csv::Reader::from_reader(reader);
    let mut insert = conn.prepare(INSERT).unwrap();

    for res in csv.deserialize() {
        let l: LinkedWordRecord = res.unwrap();
        insert.execute(params![l.link_id, l.link_type, l.first, l.second]).unwrap();
    }
}

fn sweep_tokens(conn: &Connection) {
    const DELETE: &str =
        "DELETE FROM login_tokens WHERE JULIANDAY(?1) - JULIANDAY(last_used) > ?2;";
    const TOKEN_EXPIRY_DAYS: f64 = 14.0;

    conn.prepare(DELETE)
        .unwrap()
        .execute(params![chrono::Utc::now(), TOKEN_EXPIRY_DAYS])
        .unwrap();
}
