use crate::import::{force_reindex, get_db_for_import, insert_word, link_words_alt_use};
use crate::Config;
use csv::StringRecord;
use itertools::Itertools;
use rusqlite::Connection;
use std::fs::File;
use std::io::BufReader;
use std::iter;
use std::path::Path;

pub fn import_isindebele_birds(cfg: Config, path: &Path) -> anyhow::Result<()> {
    let conn = get_db_for_import(&cfg)?;

    let reader = BufReader::new(File::open(path)?);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .delimiter(b';')
        .from_reader(reader);

    for record in reader.records() {
        let record = record?;
        process_record(&conn, record)?
    }

    force_reindex(&cfg)
}

fn process_record(conn: &Connection, record: StringRecord) -> anyhow::Result<()> {
    let english = record.get(0).unwrap().trim();
    let ndebele = record.get(1).unwrap().trim();

    let word_ids: Vec<i64> = iter::once(english)
        .cartesian_product(ndebele.split("/"))
        .map(|(english, ndebele)| insert_word(conn, english, ndebele))
        .collect();

    word_ids
        .iter()
        .combinations(2)
        .for_each(|list| link_words_alt_use(conn, *list[0], *list[1]));

    Ok(())
}
