//! This script is called daily to back up the database and sweep unused login tokens.

use crate::i18n::{I18nInfo, EN_ZA};
use crate::{set_up_db, CliArgs, Config};
use anyhow::Result;
use chrono::Utc;
use fallible_iterator::FallibleIterator;
use genanki_rs::{Deck, Field, Model, ModelType, Note, Template};
use isixhosa::noun::NounClass;
use isixhosa_common::format::DisplayHtml;
use isixhosa_common::language::{ConjunctionFollowedBy, PartOfSpeech, Transitivity, WordLinkType};
use isixhosa_common::types::{ExistingExample, ExistingWord};
use rusqlite::backup::Backup;
use rusqlite::{params, OptionalExtension};
use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::convert::TryFrom;
use std::fs::File;
use std::io;
use std::io::{BufReader, BufWriter, Write};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tempdir::TempDir;

// TODO(restore users)
pub fn restore(cfg: Config) -> Result<()> {
    let conn = Connection::open(&cfg.database_path)?;

    set_up_db(&conn)?;
    restore_words(&cfg, &conn)?;
    restore_examples(&cfg, &conn)?;
    restore_linked_words(&cfg, &conn)?;
    restore_contributions(&cfg, &conn)?;

    // Force reindex on next start
    std::fs::remove_dir_all(&cfg.tantivy_path)?;
    std::fs::create_dir_all(&cfg.tantivy_path)?;

    Ok(())
}

pub fn run_daily_tasks(cfg: &Config, args: &CliArgs) -> Result<()> {
    let conn = Connection::open(&cfg.database_path)?;
    sweep_tokens(&conn)?;
    export(&cfg, &args.site, &conn)
}

fn export(cfg: &Config, site: &str, src: &Connection) -> Result<()> {
    let temp_dir = TempDir::new("isixhosa_click_backup")?;
    let temp_db = temp_dir.path().join("isixhosa_click.bak.db");
    let mut dest = Connection::open(temp_db)?;

    {
        let backup = Backup::new(src, &mut dest)?;
        backup.run_to_completion(5, Duration::from_millis(250), None)?;
    }

    write_words(cfg, site, &dest)?;
    write_examples(cfg, &dest)?;
    write_linked_words(cfg, &dest)?;
    write_users(cfg, &dest)?;
    write_contributions(cfg, &dest)?;

    let output = Command::new("git")
        .current_dir(&cfg.plaintext_export_path)
        .args([
            "commit",
            "-a",
            "-m",
            &format!(
                "Daily backup for {}\n>\n>\non-behalf-of: @IsiXhosa-click <restiosondev@gmail.com>",
                Utc::now().date_naive()
            ),
        ])
        .output()?;

    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;

    let output = Command::new("git")
        .current_dir(&cfg.plaintext_export_path)
        .arg("push")
        .output()?;

    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;
    Ok(())
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct WordRecord {
    pub word_id: u64,

    pub english: String,
    pub xhosa: String,
    pub part_of_speech: Option<PartOfSpeech>,

    pub xhosa_tone_markings: String,
    pub infinitive: String,
    pub is_plural: bool,
    pub is_inchoative: bool,
    pub is_informal: bool,
    pub transitivity: Option<Transitivity>,
    pub followed_by: Option<ConjunctionFollowedBy>,
    pub noun_class: Option<NounClass>,
    pub note: String,
}

impl WordRecord {
    fn render_note(
        self,
        i18n_info: I18nInfo,
        en_example: String,
        xh_example: String,
    ) -> Result<(Note, Vec<String>), anyhow::Error> {
        const CSS: &str = include_str!("anki.css");

        let xhosa_up = Template::new("Card Reverse")
            .qfmt(
                r#"
                        <div class="translation">{{Xhosa}}</div>
                        <div class="extra">{{XhosaExtra}}</div>
                        {{#XhosaExample}}
                            <div class="example_header">Example</div>
                            <div class="example">{{ XhosaExample }}</div>
                        {{/XhosaExample}}
                    "#,
            )
            .afmt(
                r#"
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

                        {{#WordNote}}
                            <p class="example">{{ WordNote }}</p>
                        {{/WordNote}}
                    "#,
            );

        let english_up = Template::new("Card Default")
            .qfmt(
                r#"
                        <div class="translation">{{English}}</div>
                        <div class="extra">{{EnglishExtra}}</div>

                        {{#EnglishExample}}
                            <div class="example_header">Example</div>
                            <div class="example">{{ EnglishExample }}</div>
                        {{/EnglishExample}}
                    "#,
            )
            .afmt(
                r#"
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

                        {{#WordNote}}
                            <p class="example">{{ WordNote }}</p>
                        {{/WordNote}}
                    "#,
            );

        let model = Model::new_with_options(
            515787989,
            "IsiXhosa.click word",
            vec![
                Field::new("WordId"),
                Field::new("English"),
                Field::new("Xhosa"),
                Field::new("EnglishExtra"),
                Field::new("XhosaExtra"),
                Field::new("EnglishExample"),
                Field::new("XhosaExample"),
                Field::new("WordNote"),
            ],
            vec![english_up, xhosa_up],
            Some(CSS),
            Some(ModelType::FrontBack),
            None,
            None,
            None,
        );

        let id = self.word_id.to_string();

        let plural = if self.is_plural { "plural" } else { "" };
        let transitivity = self
            .transitivity
            .map(|t| t.to_plaintext(&i18n_info).to_string())
            .unwrap_or_default();

        let part_of_speech = self
            .part_of_speech
            .map(|p| p.to_plaintext(&i18n_info).to_string())
            .unwrap_or_default();

        let en_extra = [plural.to_owned(), transitivity.clone(), part_of_speech];

        let en_extra = Self::join_if_non_empty(&en_extra, " ");

        let class = match self.noun_class {
            Some(class) => format!("class {}", class.to_html(&i18n_info)),
            None => String::new(),
        };

        let part_of_speech = self
            .part_of_speech
            .map(|p| p.to_plaintext(&i18n_info).to_string())
            .unwrap_or_default();

        let pos_info = [
            plural.to_owned(),
            if self.is_inchoative {
                "inchoative".to_owned()
            } else {
                String::new()
            },
            transitivity,
            part_of_speech,
        ];

        let xh_extra = [
            self.xhosa_tone_markings,
            Self::join_if_non_empty(&pos_info, " "),
            self.infinitive,
            class,
            self.followed_by
                .map(|s| format!("followed by {}", s.to_plaintext(&i18n_info)))
                .unwrap_or_default(),
        ];

        let xh_extra = Self::join_if_non_empty(&xh_extra, " - ");

        let fields: Vec<String> = vec![
            id,
            self.english,
            self.xhosa,
            en_extra,
            xh_extra,
            en_example,
            xh_example,
            self.note,
        ];

        Ok((
            Note::new(model, fields.iter().map(AsRef::as_ref).collect())?,
            fields,
        ))
    }

    fn join_if_non_empty(arr: &[String], join: &str) -> String {
        let mut joined = String::new();
        let mut first = true;

        for string in arr {
            if !string.is_empty() {
                if first {
                    first = false;
                } else {
                    joined.push_str(join);
                }

                joined.push_str(string);
            }
        }

        joined
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
            is_inchoative: w.is_inchoative,
            is_informal: w.is_informal,
            transitivity: w.transitivity,
            followed_by: w.followed_by,
            noun_class: w.noun_class,
            note: w.note,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct LinkedWordRecord {
    pub link_id: u64,
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

#[derive(Serialize, Deserialize)]
pub struct ContributionRecord {
    pub word_id: u64,
    pub user_id: u64,
}

impl TryFrom<&Row<'_>> for ContributionRecord {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(ContributionRecord {
            word_id: row.get("word_id")?,
            user_id: row.get("user_id")?,
        })
    }
}

#[derive(Serialize)]
pub struct UserRecord {
    pub user_id: u64,
    pub username: String,
}

impl TryFrom<&Row<'_>> for UserRecord {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(UserRecord {
            user_id: row.get("user_id")?,
            username: row.get("username")?,
        })
    }
}

fn csv_writer(cfg: &Config, file: &str) -> Result<csv::Writer<BufWriter<File>>> {
    let path = cfg.plaintext_export_path.join(file);
    let writer = BufWriter::new(File::create(path)?);
    Ok(csv::Writer::from_writer(writer))
}

fn csv_reader(cfg: &Config, file: &str) -> Result<csv::Reader<BufReader<File>>> {
    let path = cfg.plaintext_export_path.join(file);
    let reader = BufReader::new(File::open(path)?);
    Ok(csv::Reader::from_reader(reader))
}

// TODO(translations) do per-site & translate anki
#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn write_words(cfg: &Config, site: &str, conn: &Connection) -> Result<()> {
    const ANKI_DESC: &str = "All the words on IsiXhosa.click, as of %d-%m-%Y.";

    const SELECT_WORDS: &str = "
        SELECT
            word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            is_inchoative, is_informal, transitivity, followed_by, noun_class, note
        FROM words
        ORDER BY word_id;
    ";

    const SELECT_EXAMPLE: &str = "SELECT english, xhosa FROM examples WHERE word_id = ?1 LIMIT 1;";

    let mut select_example = conn.prepare(SELECT_EXAMPLE)?;

    let wordle_path = cfg.other_static_files.join("wordle_words.csv");
    let writer = BufWriter::new(File::create(wordle_path)?);
    let mut wordle_csv = csv::Writer::from_writer(writer);

    let mut full_word_csv = csv_writer(cfg, "words.csv")?;

    let file = File::create(cfg.other_static_files.join("anki_deck.txt"))?;
    let writer = BufWriter::new(file);
    let mut plaintext_deck = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .has_headers(false)
        .from_writer(writer);

    let mut deck = Deck::new(
        1,
        "IsiXhosa.click words",
        &Utc::now().format(ANKI_DESC).to_string(),
    );

    let words: Vec<WordRecord> = conn
        .prepare(SELECT_WORDS)?
        .query(params![])?
        .map(|row| Ok(WordRecord::from(ExistingWord::try_from(row)?)))
        .collect()?;

    let ctx = crate::i18n::load(site.to_owned(), cfg);
    let i18n_info = I18nInfo {
        user_language: EN_ZA,
        ctx: Arc::new(ctx),
    };

    for word in words {
        let (en_example, xh_example): (String, String) = select_example
            .query_row(params![word.word_id], |row| {
                Ok((row.get("english")?, row.get("xhosa")?))
            })
            .optional()?
            .unwrap_or_default();

        full_word_csv.serialize(&word)?;
        wordle_csv.serialize(&word)?;

        let (note, fields) = word.render_note(i18n_info.clone(), en_example, xh_example)?;
        deck.add_note(note);
        plaintext_deck.write_record(fields)?;
    }

    deck.write_to_file(
        cfg.other_static_files
            .join("anki_deck.apkg")
            .to_str()
            .unwrap(),
    )?;

    Ok(())
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn restore_words(cfg: &Config, conn: &Connection) -> Result<()> {
    const INSERT: &str = "
        INSERT INTO words (
            word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
            is_inchoative, is_informal, transitivity, followed_by, noun_class, note
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13);
    ";

    let mut csv = csv_reader(cfg, "words.csv")?;
    let mut insert = conn.prepare(INSERT)?;

    for res in csv.deserialize() {
        let w: WordRecord = res?;

        insert.execute(params![
            w.word_id,
            w.english,
            w.xhosa,
            w.part_of_speech,
            w.xhosa_tone_markings,
            w.infinitive,
            w.is_plural,
            w.is_inchoative,
            w.is_informal,
            w.transitivity,
            w.followed_by.unwrap_or_default(),
            w.noun_class.map(|x| x as u8),
            w.note
        ])?;
    }

    Ok(())
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn write_examples(cfg: &Config, conn: &Connection) -> Result<()> {
    const SELECT: &str = "
        SELECT example_id, word_id, english, xhosa
        FROM examples
        ORDER BY example_id;
    ";

    let mut csv = csv_writer(cfg, "examples.csv")?;

    conn.prepare(SELECT)?
        .query(params![])?
        .map(|row| ExistingExample::try_from(row))
        .map_err(|e| -> anyhow::Error { e.into() })
        .for_each(|example| csv.serialize(example).map_err(Into::into))
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn restore_examples(cfg: &Config, conn: &Connection) -> Result<()> {
    const INSERT: &str = "
        INSERT INTO examples (example_id, word_id, english, xhosa) VALUES (?1, ?2, ?3, ?4);
    ";

    let mut csv = csv_reader(cfg, "examples.csv")?;
    let mut insert = conn.prepare(INSERT)?;

    for res in csv.deserialize() {
        let e: ExistingExample = res?;
        insert.execute(params![e.example_id, e.word_id, e.english, e.xhosa])?;
    }

    Ok(())
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn write_linked_words(cfg: &Config, conn: &Connection) -> Result<()> {
    const SELECT: &str = "
        SELECT link_id, link_type, first_word_id, second_word_id
        FROM linked_words
        ORDER BY link_id;
    ";

    let mut csv = csv_writer(cfg, "linked_words.csv")?;

    conn.prepare(SELECT)?
        .query(params![])?
        .map(|row| LinkedWordRecord::try_from(row))
        .map_err(|e| -> anyhow::Error { e.into() })
        .for_each(|example| csv.serialize(example).map_err(Into::into))
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn restore_linked_words(cfg: &Config, conn: &Connection) -> Result<()> {
    const INSERT: &str = "
        INSERT INTO linked_words
            (link_id, link_type, first_word_id, second_word_id)
        VALUES (?1, ?2, ?3, ?4);
    ";

    let mut csv = csv_reader(cfg, "linked_words.csv")?;
    let mut insert = conn.prepare(INSERT)?;

    for res in csv.deserialize() {
        let l: LinkedWordRecord = res?;
        insert.execute(params![l.link_id, l.link_type, l.first, l.second])?;
    }

    Ok(())
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn write_contributions(cfg: &Config, conn: &Connection) -> Result<()> {
    const SELECT: &str = "
        SELECT
            user_attributions.word_id, user_attributions.user_id
        FROM user_attributions
        INNER JOIN users ON user_attributions.user_id = users.user_id
        WHERE users.display_name = 1
        ORDER BY word_id;
    ";

    let mut csv = csv_writer(cfg, "user_attributions.csv")?;

    conn.prepare(SELECT)?
        .query(params![])?
        .map(|row| ContributionRecord::try_from(row))
        .map_err(|e| -> anyhow::Error { e.into() })
        .for_each(|example| csv.serialize(example).map_err(Into::into))?;

    Ok(())
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn restore_contributions(cfg: &Config, conn: &Connection) -> Result<()> {
    const INSERT: &str = "
        INSERT INTO user_attributions (word_id, user_id) VALUES (?1, ?2);
    ";

    let mut csv = csv_reader(cfg, "user_attributions.csv")?;
    let mut insert = conn.prepare(INSERT)?;

    for res in csv.deserialize() {
        let l: ContributionRecord = res?;
        insert.execute(params![l.word_id, l.user_id])?;
    }

    Ok(())
}

#[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
fn write_users(cfg: &Config, conn: &Connection) -> Result<()> {
    const SELECT: &str =
        "SELECT user_id, username FROM users WHERE display_name = 1 ORDER BY user_id;";

    let mut csv = csv_writer(cfg, "users.csv")?;

    conn.prepare(SELECT)?
        .query(params![])?
        .map(|row| UserRecord::try_from(row))
        .map_err(|e| -> anyhow::Error { e.into() })
        .for_each(|example| csv.serialize(example).map_err(Into::into))?;

    Ok(())
}

fn sweep_tokens(conn: &Connection) -> Result<()> {
    const DELETE: &str =
        "DELETE FROM login_tokens WHERE JULIANDAY(?1) - JULIANDAY(last_used) > ?2;";
    const TOKEN_EXPIRY_DAYS: f64 = 30.0;

    conn.prepare(DELETE)?
        .execute(params![Utc::now(), TOKEN_EXPIRY_DAYS])?;

    Ok(())
}
