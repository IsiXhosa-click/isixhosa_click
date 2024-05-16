use crate::language::{
    ConjunctionFollowedBy, NounClassExt, PartOfSpeech, Transitivity, WordLinkType,
};
use crate::serialization::{DiscrimOutOfRange, SerAndDisplayWithDisplayHtml, WithDeleteSentinel};
use crate::types::{ExistingExample, ExistingLinkedWord, ExistingWord, PublicUserInfo, WordHit};
use fallible_iterator::FallibleIterator;
use isixhosa::noun::NounClass;
use num_enum::TryFromPrimitive;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef};
use rusqlite::{params, Row};
use rusqlite::{OptionalExtension, ToSql};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;
use std::str::FromStr;
use tracing::{instrument, Span};

#[derive(Clone)]
pub struct DbBase(pub Pool<SqliteConnectionManager>);

impl DbBase {
    pub fn new(pool: Pool<SqliteConnectionManager>) -> DbBase {
        DbBase(pool)
    }
}

pub mod db_impl {
    use super::*;
    use r2d2::PooledConnection;
    use r2d2_sqlite::SqliteConnectionManager;

    #[derive(Clone)]
    pub struct DbImpl(pub Pool<SqliteConnectionManager>);

    impl PublicAccessDb for DbImpl {
        fn get(&self) -> Result<PooledConnection<SqliteConnectionManager>, r2d2::Error> {
            self.0.get()
        }
    }

    impl UserAccessDb for DbImpl {}
    impl ModeratorAccessDb for DbImpl {}
}

pub trait PublicAccessDb: Clone + Send + Sync + 'static {
    fn get(&self) -> Result<PooledConnection<SqliteConnectionManager>, r2d2::Error>;
}

pub trait UserAccessDb: PublicAccessDb {}
pub trait ModeratorAccessDb: UserAccessDb {}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WordOrSuggestionId {
    ExistingWord { existing_id: u64 },
    Suggested { suggestion_id: u64 },
}

impl From<WordId> for WordOrSuggestionId {
    fn from(id: WordId) -> Self {
        WordOrSuggestionId::ExistingWord { existing_id: id.0 }
    }
}

impl WordOrSuggestionId {
    pub fn suggested(id: u64) -> WordOrSuggestionId {
        WordOrSuggestionId::Suggested { suggestion_id: id }
    }

    pub fn existing(id: u64) -> WordOrSuggestionId {
        WordOrSuggestionId::ExistingWord { existing_id: id }
    }

    pub fn into_existing(self) -> Option<u64> {
        match self {
            WordOrSuggestionId::ExistingWord { existing_id } => Some(existing_id),
            _ => None,
        }
    }

    pub fn into_suggested(self) -> Option<u64> {
        match self {
            WordOrSuggestionId::Suggested { suggestion_id } => Some(suggestion_id),
            _ => None,
        }
    }

    pub fn is_existing(&self) -> bool {
        matches!(self, WordOrSuggestionId::ExistingWord { .. })
    }

    pub fn is_suggested(&self) -> bool {
        !self.is_existing()
    }

    pub fn inner(&self) -> u64 {
        match self {
            WordOrSuggestionId::ExistingWord { existing_id } => *existing_id,
            WordOrSuggestionId::Suggested { suggestion_id } => *suggestion_id,
        }
    }

    fn try_from_row(
        row: &Row<'_>,
        existing_idx: &str,
        suggested_idx: &str,
    ) -> Result<WordOrSuggestionId, rusqlite::Error> {
        let existing_word_id: Option<u64> = row
            .get::<&str, Option<i64>>(existing_idx)
            .unwrap()
            .map(|x| x as u64);
        let suggested_word_id: Option<u64> = row
            .get::<&str, Option<i64>>(suggested_idx)
            .unwrap()
            .map(|x| x as u64);
        match (existing_word_id, suggested_word_id) {
            (Some(existing_id), None) => Ok(WordOrSuggestionId::existing(existing_id)),
            (None, Some(suggestion_id)) => Ok(WordOrSuggestionId::suggested(suggestion_id)),
            (existing, _suggested) => {
                panic!(
                    "Invalid pair of existing/suggested ids: existing - {:?} suggested - {:?}",
                    existing, suggested_word_id
                )
            }
        }
    }
}

impl TryFrom<&Row<'_>> for WordOrSuggestionId {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        WordOrSuggestionId::try_from_row(row, "existing_word_id", "suggested_word_id")
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct WordId(pub u64);

impl ExistingExample {
    #[instrument(
        level = "trace",
        name = "Fetch all existing examples for word",
        fields(results),
        skip(db)
    )]
    pub fn fetch_all_for_word(db: &impl PublicAccessDb, word_id: u64) -> Vec<ExistingExample> {
        const SELECT: &str =
            "SELECT example_id, word_id, english, xhosa FROM examples WHERE word_id = ?1;";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let rows = query.query(params![word_id]).unwrap();

        #[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
        let examples: Vec<Self> = rows
            .map(|row| ExistingExample::try_from(row))
            .collect()
            .unwrap();

        Span::current().record("results", examples.len());

        examples
    }

    #[instrument(
        level = "trace",
        name = "Fetch existing example",
        fields(found),
        skip(db)
    )]
    pub fn fetch(db: &impl PublicAccessDb, example_id: u64) -> Option<ExistingExample> {
        const SELECT: &str =
            "SELECT example_id, word_id, english, xhosa FROM examples WHERE example_id = ?1;";

        let conn = db.get().unwrap();
        #[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
        let opt = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![example_id], |row| ExistingExample::try_from(row))
            .optional()
            .unwrap();

        Span::current().record("found", opt.is_some());

        opt
    }
}

impl TryFrom<&Row<'_>> for ExistingExample {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(ExistingExample {
            example_id: row.get("example_id")?,
            word_id: row.get("word_id")?,
            english: row.get("english")?,
            xhosa: row.get("xhosa")?,
        })
    }
}

impl ExistingLinkedWord {
    #[instrument(
        level = "trace",
        name = "Fetch all existing linked word for word",
        fields(results),
        skip(db)
    )]
    pub fn fetch_all_for_word(db: &impl PublicAccessDb, word_id: u64) -> Vec<ExistingLinkedWord> {
        const SELECT: &str = "
            SELECT link_id, link_type, first_word_id, second_word_id FROM linked_words
                WHERE first_word_id = ?1 OR second_word_id = ?1
        ";

        let conn = db.get().unwrap();
        let mut query = conn.prepare(SELECT).unwrap();
        let rows = query.query(params![word_id]).unwrap();

        let mut vec: Vec<ExistingLinkedWord> = rows
            .map(|row| ExistingLinkedWord::try_from_row_populate_other(row, db, word_id))
            .collect()
            .unwrap();

        Span::current().record("results", vec.len());

        vec.sort_by_key(|l| l.link_type);
        vec
    }

    #[instrument(
        level = "trace",
        name = "Fetch existing linked word",
        fields(found),
        skip(db)
    )]
    pub fn fetch(
        db: &impl PublicAccessDb,
        id: u64,
        skip_populating: u64,
    ) -> Option<ExistingLinkedWord> {
        const SELECT: &str = "
            SELECT link_id, link_type, first_word_id, second_word_id FROM linked_words
                WHERE link_id = ?1;
        ";

        let conn = db.get().unwrap();
        let opt = conn
            .prepare(SELECT)
            .unwrap()
            .query_row(params![id], |row| {
                ExistingLinkedWord::try_from_row_populate_other(row, db, skip_populating)
            })
            .optional()
            .unwrap();

        Span::current().record("found", opt.is_some());

        opt
    }

    #[instrument(name = "Populate existing linked word", fields(link_id), skip(row, db))]
    pub fn try_from_row_populate_other(
        row: &Row<'_>,
        db: &impl PublicAccessDb,
        skip_populating: u64,
    ) -> Result<Self, rusqlite::Error> {
        let (first_word_id, second_word_id) =
            (row.get("first_word_id")?, row.get("second_word_id")?);
        let populate = if first_word_id != skip_populating {
            first_word_id
        } else {
            second_word_id
        };

        let link_id = row.get("link_id")?;

        Span::current().record("link_id", link_id);

        Ok(ExistingLinkedWord {
            link_id,
            first_word_id,
            second_word_id,
            link_type: row.get("link_type")?,
            other: WordHit::fetch_from_db(db, WordOrSuggestionId::existing(populate)).unwrap(),
        })
    }
}

impl ExistingWord {
    #[instrument(name = "Fetch full existing word", fields(found), skip(db))]
    pub fn fetch_full(db: &impl PublicAccessDb, id: u64) -> Option<ExistingWord> {
        let mut word = ExistingWord::fetch_alone(db, id);
        if let Some(word) = word.as_mut() {
            word.examples = ExistingExample::fetch_all_for_word(db, id);
            word.linked_words = ExistingLinkedWord::fetch_all_for_word(db, id);
            word.contributors = PublicUserInfo::fetch_public_contributors_for_word(db, id);
        }

        Span::current().record("found", word.is_some());

        word
    }

    #[instrument(
        level = "trace",
        name = "Fetch just existing word",
        fields(found),
        skip(db)
    )]
    pub fn fetch_alone(db: &impl PublicAccessDb, id: u64) -> Option<ExistingWord> {
        const SELECT_ORIGINAL: &str = "
            SELECT
                word_id, english, xhosa, part_of_speech, xhosa_tone_markings, infinitive, is_plural,
                is_inchoative, is_informal, transitivity, followed_by, noun_class, note
            FROM words
            WHERE word_id = ?1;
        ";

        let conn = db.get().unwrap();

        #[allow(clippy::redundant_closure)] // "implementation of FnOnce is not general enough"
        let opt = conn
            .prepare(SELECT_ORIGINAL)
            .unwrap()
            .query_row(params![id], |row| ExistingWord::try_from(row))
            .optional()
            .unwrap();

        Span::current().record("found", opt.is_some());

        opt
    }

    #[instrument(name = "Delete existing word", fields(found), skip(db))]
    pub fn delete(db: &impl ModeratorAccessDb, id: u64) -> bool {
        const DELETE: &str = "DELETE FROM words WHERE word_id = ?1;";

        let conn = db.get().unwrap();
        let modified_rows = conn.prepare(DELETE).unwrap().execute(params![id]).unwrap();
        let found = modified_rows == 1;
        Span::current().record("found", found);
        found
    }

    #[instrument(name = "Count all existing words", fields(results), skip(db))]
    pub fn count_all(db: &impl PublicAccessDb) -> u64 {
        const COUNT: &str = "SELECT COUNT(1) FROM words;";

        let conn = db.get().unwrap();
        let count = conn
            .prepare(COUNT)
            .unwrap()
            .query_row(params![], |row| row.get(0))
            .unwrap();

        Span::current().record("results", count);

        count
    }
}

impl WordHit {
    pub fn try_from_row_and_id(
        row: &Row<'_>,
        id: WordOrSuggestionId,
    ) -> Result<WordHit, rusqlite::Error> {
        Ok(WordHit {
            id: id.inner(),
            english: row.get("english")?,
            xhosa: row.get("xhosa")?,
            part_of_speech: SerAndDisplayWithDisplayHtml(row.get("part_of_speech")?),
            is_plural: row.get("is_plural")?,
            is_inchoative: row.get("is_inchoative")?,
            is_informal: row.get("is_informal")?,
            transitivity: row
                .get_with_sentinel("transitivity")?
                .map(SerAndDisplayWithDisplayHtml),
            is_suggestion: id.is_suggested(),
            noun_class: row
                .get_with_sentinel("noun_class")?
                .map(|c: NounClass| c.to_prefixes()),
        })
    }

    #[instrument(
        level = "trace",
        name = "Fetch word hit from database",
        fields(found),
        skip(db)
    )]
    pub fn fetch_from_db(db: &impl PublicAccessDb, id: WordOrSuggestionId) -> Option<WordHit> {
        const SELECT_EXISTING: &str = "
            SELECT
                english, xhosa, part_of_speech, is_plural, is_inchoative, is_informal, transitivity, noun_class
            FROM words
            WHERE word_id = ?1;
        ";
        const SELECT_SUGGESTED: &str = "
            SELECT
                english, xhosa, part_of_speech, is_plural, is_inchoative, is_informal, transitivity, noun_class,
                username, display_name, suggesting_user
            FROM word_suggestions
            INNER JOIN users ON word_suggestions.suggesting_user = users.user_id
            WHERE suggestion_id = ?1;
        ";

        let conn = db.get().unwrap();

        let stmt = match id {
            WordOrSuggestionId::ExistingWord { .. } => SELECT_EXISTING,
            WordOrSuggestionId::Suggested { .. } => SELECT_SUGGESTED,
        };

        // WTF rustc?
        #[allow(clippy::redundant_closure)] // implementation of FnOnce is not general enough
        let v = conn
            .prepare(stmt)
            .unwrap()
            .query_row(params![id.inner()], |row| {
                WordHit::try_from_row_and_id(row, id)
            })
            .optional()
            .unwrap();

        Span::current().record("found", v.is_some());

        v
    }
}

impl PublicUserInfo {
    pub fn fetch_public_contributors_for_word(
        db: &impl PublicAccessDb,
        word: u64,
    ) -> Vec<PublicUserInfo> {
        const SELECT: &str = "
            SELECT users.user_id as suggesting_user, users.username, display_name
            FROM user_attributions
            INNER JOIN users ON users.user_id = user_attributions.user_id
            WHERE word_id = ?1 AND users.display_name = 1;
        ";

        let conn = db.get().unwrap();

        let mut query = conn.prepare(SELECT).unwrap();

        #[allow(clippy::redundant_closure)] // lifetime issue
        query
            .query(params![word])
            .unwrap()
            .map(|row| PublicUserInfo::try_from(row))
            .collect()
            .unwrap()
    }
}

impl TryFrom<&Row<'_>> for PublicUserInfo {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        Ok(PublicUserInfo {
            id: NonZeroU64::new(row.get::<&str, u64>("suggesting_user")?).unwrap(),
            username: row.get("username")?,
            display_name: row.get("display_name")?,
        })
    }
}

impl TryFrom<&Row<'_>> for ExistingWord {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, rusqlite::Error> {
        Ok(ExistingWord {
            word_id: row.get("word_id")?,
            english: row.get("english")?,
            xhosa: row.get("xhosa")?,
            part_of_speech: row.get("part_of_speech")?,
            xhosa_tone_markings: row.get("xhosa_tone_markings")?,
            infinitive: row.get("infinitive")?,
            is_plural: row.get("is_plural")?,
            is_inchoative: row.get("is_inchoative")?,
            transitivity: row.get_with_sentinel("transitivity")?,
            followed_by: ConjunctionFollowedBy::from_str(&row.get::<&str, String>("followed_by")?)
                .ok(),
            noun_class: row.get_with_sentinel("noun_class")?,
            note: row.get("note")?,
            is_informal: row.get("is_informal")?,
            examples: vec![],
            linked_words: vec![],
            contributors: vec![],
        })
    }
}

impl FromSql for PartOfSpeech {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        let err = || FromSqlError::Other(Box::new(DiscrimOutOfRange(v, "PartOfSpeech")));
        Self::try_from_primitive(v.try_into().map_err(|_| err())?).map_err(|_| err())
    }
}

impl ToSql for PartOfSpeech {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as u8 as i64)))
    }
}

impl ToSql for ConjunctionFollowedBy {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(ValueRef::Text(
            self.as_ref().as_bytes(),
        )))
    }
}

impl ToSql for Transitivity {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer((*self as u8) as i64)))
    }
}

impl FromSql for Transitivity {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        let err = || FromSqlError::Other(Box::new(DiscrimOutOfRange(v, "Transitivity")));
        Self::try_from_primitive(v.try_into().map_err(|_| err())?).map_err(|_| err())
    }
}

impl FromSql for WordLinkType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        let err = || FromSqlError::Other(Box::new(DiscrimOutOfRange(v, "WordLinkType")));
        Self::try_from_primitive(v.try_into().map_err(|_| err())?).map_err(|_| err())
    }
}

impl ToSql for WordLinkType {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, rusqlite::Error> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as u8 as i64)))
    }
}

impl<T> FromSql for WithDeleteSentinel<T>
where
    T: TryFromPrimitive,
    T::Primitive: TryFrom<i64>,
{
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;

        if v == 255 {
            Ok(WithDeleteSentinel::Remove)
        } else {
            let err =
                || FromSqlError::Other(Box::new(DiscrimOutOfRange(v, std::any::type_name::<T>())));
            T::try_from_primitive(v.try_into().map_err(|_| err())?)
                .map_err(|_| err())
                .map(WithDeleteSentinel::Some)
        }
    }
}

pub trait GetWithSentinelExt<T> {
    fn get_with_sentinel(&self, idx: &str) -> rusqlite::Result<Option<T>>;
}

impl<'a, T> GetWithSentinelExt<T> for Row<'a>
where
    T: TryFromPrimitive,
    T::Primitive: TryFrom<i64>,
{
    fn get_with_sentinel(&self, idx: &str) -> rusqlite::Result<Option<T>> {
        let opt = self.get::<&str, Option<WithDeleteSentinel<T>>>(idx)?;
        Ok(opt.and_then(|x| match x {
            WithDeleteSentinel::Some(v) => Some(v),
            WithDeleteSentinel::Remove => None,
        }))
    }
}
