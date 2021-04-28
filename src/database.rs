//! TODO(cleanup) refactor to put all DB stuff here or in a module under here

use crate::typesense::WordHit;
use r2d2_sqlite::SqliteConnectionManager;
use r2d2::Pool;
use crate::language::SerializeDisplay;
use rusqlite::{params, OptionalExtension};

pub fn get_word_hit_from_db(db: Pool<SqliteConnectionManager>, id: i32) -> Option<WordHit> {
    const SELECT: &str =
        "SELECT english, xhosa, part_of_speech, is_plural, noun_class from words
            WHERE word_id = ?1;";

    let conn = db.get().unwrap();

    // WTF rustc?
    let v = conn.prepare(SELECT).unwrap()
        .query_row(params![id], |row| {
            Ok(WordHit {
                id: id.to_string(),
                english: row.get("english").unwrap(),
                xhosa: row.get("xhosa").unwrap(),
                part_of_speech: SerializeDisplay(row.get("part_of_speech").unwrap()),
                is_plural: row.get("is_plural").unwrap(),
                noun_class: row.get::<&str, Option<_>>("noun_class").unwrap().map(SerializeDisplay),
            })
        })
        .optional()
        .unwrap();
    v
}
