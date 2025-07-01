use crate::import::{
    force_reindex, get_db_for_import, insert_dataset_attribution_suggestions,
    insert_suggested_word_with_info, link_word_suggestions_alt_use,
};
use crate::Config;
use csv::StringRecord;
use isixhosa::noun::NounClass;
use isixhosa_common::language::{PartOfSpeech, Transitivity};
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub fn import_medical_glossary(cfg: Config, path: &Path, dataset_id: i64) -> anyhow::Result<()> {
    let conn = get_db_for_import(&cfg)?;

    let reader = BufReader::new(File::open(path)?);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .delimiter(b';')
        .from_reader(reader);

    let mut words_by_english = HashMap::new();
    let mut words_by_xhosa = HashMap::new();

    for record in reader.records() {
        let record = record?;
        let (en, xh, id) = process_record(&conn, record, dataset_id)?;

        if let Some(other_id) = words_by_english.get(&en) {
            link_word_suggestions_alt_use(&conn, 1, id, *other_id);
        } else {
            words_by_english.insert(en, id);
        }

        if let Some(other_id) = words_by_xhosa.get(&xh) {
            link_word_suggestions_alt_use(&conn, 1, id, *other_id);
        } else {
            words_by_xhosa.insert(xh, id);
        }
    }

    force_reindex(&cfg)
}

fn process_record(
    conn: &Connection,
    record: StringRecord,
    dataset_id: i64,
) -> anyhow::Result<(String, String, i64)> {
    let english = record.get(0).unwrap().trim();
    let xhosa = record.get(1).unwrap().trim();

    let part_of_speech = record.get(2).unwrap().trim();
    let (part_of_speech, noun_class) = match record.get(2).unwrap().trim() {
        "ADV" => (PartOfSpeech::Adverb, None),
        "V" => (PartOfSpeech::Verb, None),
        pos if pos.starts_with("N") => {
            let class = match pos.strip_prefix("N").unwrap() {
                "1" => NounClass::Class1Um,
                "2" => NounClass::Aba,
                "1a" => NounClass::U,
                "2a" => NounClass::Oo,
                "3" => NounClass::Class3Um,
                "4" => NounClass::Imi,
                "5" => NounClass::Ili,
                "6" => NounClass::Ama,
                "7" => NounClass::Isi,
                "8" => NounClass::Izi,
                "9" => NounClass::In,
                "10" => NounClass::Izin,
                "11" => NounClass::Ulu,
                "14" => NounClass::Ubu,
                "15" => NounClass::Uku,
                _ => anyhow::bail!("Unknown noun class {pos} for record {record:?}"),
            };

            (PartOfSpeech::Noun, Some(class))
        }
        _ => anyhow::bail!("Unknown part of speech {part_of_speech} for record {record:?}"),
    };
    let is_plural = record.get(3).unwrap().trim().to_lowercase() == "plural";
    let infinitive = record.get(4).unwrap().trim();
    let is_inchoative = record.get(5).unwrap().trim().to_lowercase() == "yes";

    let transitivity = record.get(6).unwrap().trim();
    let transitivity = match record.get(6).unwrap().trim() {
        "Transitive" => Transitivity::Transitive,
        "Intransitive" => Transitivity::Intransitive,
        "" => Transitivity::Ambitransitive,
        _ => anyhow::bail!("Unknown transitivity {transitivity} for record {record:?}"),
    };
    let transitivity = Some(transitivity).filter(|_| part_of_speech == PartOfSpeech::Verb);

    let note = record.get(7).unwrap().trim();

    let suggestion_id = insert_suggested_word_with_info(
        conn,
        english,
        xhosa,
        infinitive,
        is_plural,
        is_inchoative,
        Some(part_of_speech),
        noun_class,
        note,
        transitivity,
    );

    insert_dataset_attribution_suggestions(conn, suggestion_id, dataset_id)?;

    Ok((english.to_owned(), xhosa.to_owned(), suggestion_id))
}
