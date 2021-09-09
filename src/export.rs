//! This script is called daily to back up the database and sweep unused login tokens.

use crate::database::existing::{ExistingExample, ExistingWord};
use crate::language::{PartOfSpeech, WordLinkType, NounClassExt};
use crate::serialization::{deser_from_str, ser_to_debug};
use crate::{Config, set_up_db};
use fallible_iterator::FallibleIterator;
use isixhosa::noun::NounClass;
use rusqlite::backup::Backup;
use rusqlite::{params, OptionalExtension};
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
use genanki_rs::{Deck, Note, Model, Field, Template, ModelType};

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
    export(&cfg, &conn);
}

fn export(cfg: &Config, src: &Connection) {
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

impl WordRecord {
    fn render_note(
        &self,
        en_example: String,
        xh_example: String
    ) -> Result<[Note; 2], anyhow::Error> {
        const CSS: &str = include_str!("anki.css");

        let english_up = Model::new_with_options(
            515787989,
            "English up",
            vec![
                Field::new("NoteId"),
                Field::new("Id"),
                Field::new("English"),
                Field::new("Xhosa"),
                Field::new("EnglishExtra"),
                Field::new("XhosaExtra"),
                Field::new("EnglishExample"),
                Field::new("XhosaExample"),
            ],
            vec![
                Template::new("Card Default")
                    .qfmt(r#"
                        <div class="translation">{{English}}</div>
                        <div class="extra">{{EnglishExtra}}</div>

                        {{#EnglishExample}}
                            <div class="example_header">Example</div>
                            <div class="example">{{ EnglishExample }}</div>
                        {{/EnglishExample}}
                    "#)
                    .afmt(r#"
                        <div class="translation">{{English}}</div>

                        <hr id="answer">

                        <div class="translation">
                            <a href="https://isixhosa.click/word/{{WordId}}">{{Xhosa}}</a>
                        </div>

                        <div class="extra">{{XhosaExtra}}</div>

                        {{#EnglishExample}}
                            <div class="example_header">Example</div>
                            <div class="example">{{ EnglishExample }}</div>
                            <div class="example">{{ XhosaExample }}</div>
                        {{/EnglishExample}}
                    "#)
            ],
            Some(CSS),
            Some(ModelType::FrontBack),
            None,
            None,
            None,
        );

        let xhosa_up = Model::new_with_options(
            558368395,
            "Xhosa up",
            vec![
                Field::new("NoteId"),
                Field::new("WordId"),
                Field::new("English"),
                Field::new("Xhosa"),
                Field::new("EnglishExtra"),
                Field::new("XhosaExtra"),
                Field::new("EnglishExample"),
                Field::new("XhosaExample"),

            ],
            vec![
                Template::new("Card Reverse")
                    .qfmt(r#"
                        <div class="translation">{{Xhosa}}</div>
                        <div class="extra">{{XhosaExtra}}</div>
                        {{#XhosaExample}}
                            <div class="example_header">Example</div>
                            <div class="example">{{ XhosaExample }}</div>
                        {{/XhosaExample}}
                    "#)
                    .afmt(r#"
                        <div class="translation">{{Xhosa}}</div>
                        <div class="extra">{{XhosaExtra}}</div>

                        <hr id="answer">

                        <div class="translation">
                            <a href="https://isixhosa.click/word/{{WordId}}">{{English}}</a>
                        </div>

                        {{#EnglishExample}}
                            <div class="example_header">Example</div>
                            <div class="example">{{ EnglishExample }}</div>
                            <div class="example">{{ XhosaExample }}</div>
                        {{/EnglishExample}}
                    "#)
            ],
            Some(CSS),
            Some(ModelType::FrontBack),
            None,
            None,
            None,
        );

        let id = self.word_id.to_string();

        let plural = if self.is_plural {
            "plural "
        } else {
            ""
        };

        let en_extra = format!("{}{}", plural, self.part_of_speech);

        let class = match self.noun_class {
            Some(class) => {
                let prefixes = class.to_prefixes();

                if self.is_plural {
                    format!(
                        "class {}<strong>{}</strong>",
                        prefixes.singular,
                        prefixes.plural.unwrap_or("undefined")
                    )
                } else {
                    let plural_part = match prefixes.plural {
                        Some(plural) => format!("/{}", plural),
                        None => String::new(),
                    };

                    format!("class <strong>{}</strong>{}", prefixes.singular, plural_part)
                }
            }
            None => String::new(),
        };

        let class_formatted = if class.len() > 0 {
            format!(" - {}", class)
        } else {
            String::new()
        };

        let xh_extra = format!("{}{}{}", plural, self.part_of_speech, class_formatted);
        let en_id = format!("{}En", id);
        let xh_id = format!("{}Xh", id);

        let xh_fields: Vec<&str> = vec![
            &xh_id,
            &id,
            &self.english,
            &self.xhosa,
            &en_extra,
            &xh_extra,
            &en_example,
            &xh_example
        ];

        let mut en_fields = xh_fields.clone();
        en_fields[0] = &en_id;

        Ok([
            Note::new(english_up, en_fields)?,
            Note::new(xhosa_up, xh_fields)?
        ])
    }
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
    const ANKI_DESC: &str = "All the words on IsiXhosa.click, as of %d-%m-%Y.";

    const SELECT_WORDS: &str = "
        SELECT
            word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            noun_class, note
        FROM words
        ORDER BY word_id;
    ";

    const SELECT_EXAMPLE: &str = "SELECT english, xhosa FROM examples WHERE word_id = ?1 LIMIT 1;";

    let mut select_example = conn.prepare(SELECT_EXAMPLE).unwrap();

    let path = cfg.plaintext_export_path.join("words.csv");
    let writer = BufWriter::new(File::create(path).unwrap());
    let mut csv = csv::Writer::from_writer(writer);

    let mut deck = Deck::new(
        1,
        "IsiXhosa.click words",
        &Utc::now().format(ANKI_DESC).to_string()
    );

    let words: Vec<WordRecord> = conn.prepare(SELECT_WORDS)
        .unwrap()
        .query(params![])
        .unwrap()
        .map(|row| Ok(WordRecord::from(ExistingWord::try_from(row)?)))
        .collect()
        .unwrap();

    for word in words {
        let (en_example, xh_example): (String, String) = select_example
            .query_row(
                params![word.word_id],
                |row| Ok((row.get("english")?, row.get("xhosa")?)),
            )
            .optional()
            .unwrap()
            .unwrap_or_default();

        let [note_1, note_2] = word.render_note(en_example, xh_example).unwrap();
        deck.add_note(note_1);
        deck.add_note(note_2);

        csv.serialize(word).unwrap();
    }

    deck.write_to_file(cfg.other_static_files.join("anki_deck.apkg").to_str().unwrap()).unwrap();
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
