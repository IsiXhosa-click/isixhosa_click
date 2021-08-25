use crate::serialization::DiscrimOutOfRange;
use isixhosa::noun::NounClass;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use r2d2_sqlite::rusqlite::types::{FromSqlResult, Value, ValueRef};
use rusqlite::types::{FromSql, FromSqlError, ToSqlOutput};
use rusqlite::ToSql;
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::convert::TryInto;
use std::fmt::{self, Debug, Display, Formatter};

#[derive(
    IntoPrimitive,
    TryFromPrimitive,
    Serialize_repr,
    Deserialize_repr,
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum PartOfSpeech {
    Verb = 1,
    Noun = 2,
    Adjective = 3,
    Adverb = 4,
    Relative = 5,
    Interjection = 6,
    Conjunction = 7,
    Preposition = 8,
    Other = 9,
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

impl PartOfSpeech {
    /// Used in askama templates
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }
}

impl Display for PartOfSpeech {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_lowercase())
    }
}

/// Noun class prefixes with singular and plural
pub struct NounClassPrefixes {
    pub singular: &'static str,
    pub plural: Option<&'static str>,
}

impl NounClassPrefixes {
    fn from_singular_plural(singular: &'static str, plural: &'static str) -> Self {
        NounClassPrefixes {
            singular,
            plural: Some(plural),
        }
    }

    fn singular_class(singular: &'static str) -> Self {
        NounClassPrefixes {
            singular,
            plural: None,
        }
    }
}

pub trait NounClassExt {
    fn to_prefixes(&self) -> NounClassPrefixes;
    fn to_u8(&self) -> u8;
}

impl NounClassExt for NounClass {
    fn to_prefixes(&self) -> NounClassPrefixes {
        use NounClass::*;

        let both = NounClassPrefixes::from_singular_plural;
        let singular = NounClassPrefixes::singular_class;

        match self {
            Class1Um | Aba => both("um", "aba"),
            U | Oo => both("u", "oo"),
            Class3Um | Imi => both("um", "imi"),
            Ili | Ama => both("i(li)", "ama"),
            Isi | Izi => both("isi", "izi"),
            In | Izin => both("i(n)", "i(z)in"),
            Ulu => singular("ulu"),
            Ubu => singular("ubu"),
            Uku => singular("uku"),
        }
    }

    /// Used in askama templates
    fn to_u8(&self) -> u8 {
        *self as u8
    }
}

#[derive(
    IntoPrimitive,
    TryFromPrimitive,
    Serialize_repr,
    Deserialize_repr,
    Copy,
    Clone,
    Debug,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum WordLinkType {
    PluralOrSingular = 1,
    Synonym = 2,
    Antonym = 3,
    Related = 4,
    Confusable = 5,
}

impl WordLinkType {
    fn to_str(&self) -> &'static str {
        match self {
            WordLinkType::PluralOrSingular => "Plural or singular form",
            WordLinkType::Synonym => "Synonym",
            WordLinkType::Antonym => "Antonym",
            WordLinkType::Related => "Related",
            WordLinkType::Confusable => "Confusable",
        }
    }

    pub fn to_u8(&self) -> u8 {
        *self as u8
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

impl Display for WordLinkType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}
