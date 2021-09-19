use crate::serialization::DiscrimOutOfRange;
use isixhosa::noun::NounClass;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use r2d2_sqlite::rusqlite::types::{FromSqlResult, Value, ValueRef};
use rusqlite::types::{FromSql, FromSqlError, ToSqlOutput};
use rusqlite::ToSql;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::fmt::{self, Debug, Display, Formatter};
use std::str::FromStr;
use strum::EnumString;

#[derive(
    IntoPrimitive,
    TryFromPrimitive,
    Serialize,
    Deserialize,
    EnumString,
    Copy,
    Clone,
    Debug,
    Hash,
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
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

impl Display for PartOfSpeech {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            PartOfSpeech::Relative => "relative (adjective)".to_owned(),
            PartOfSpeech::Adjective => "adjective - isiphawuli".to_owned(),
            _ => format!("{:?}", self).to_lowercase(),
        };

        f.write_str(&s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConjunctionFollowedBy {
    Indicative,
    Subjunctive,
    Participial,
    Custom(String),
}

impl Default for ConjunctionFollowedBy {
    fn default() -> Self {
        ConjunctionFollowedBy::Custom(String::new())
    }
}

impl AsRef<str> for ConjunctionFollowedBy {
    fn as_ref(&self) -> &str {
        match self {
            ConjunctionFollowedBy::Indicative => "indicative mood",
            ConjunctionFollowedBy::Subjunctive => "subjunctive mood",
            ConjunctionFollowedBy::Participial => "participial mood",
            ConjunctionFollowedBy::Custom(s) => s,
        }
    }
}

impl Display for ConjunctionFollowedBy {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_ref())
    }
}

pub struct ConjunctionFollowedByNone;

impl Display for ConjunctionFollowedByNone {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("ConjunctionFollowedBy is none")
    }
}

impl FromStr for ConjunctionFollowedBy {
    type Err = ConjunctionFollowedByNone;

    fn from_str(s: &str) -> Result<Self, ConjunctionFollowedByNone> {
        let s = s.trim().to_lowercase();
        Ok(match &s[..] {
            "indicative mood" | "indicative" => ConjunctionFollowedBy::Indicative,
            "subjunctive mood" | "subjunctive" => ConjunctionFollowedBy::Subjunctive,
            "participial mood" | "participial" => ConjunctionFollowedBy::Participial,
            "" => return Err(ConjunctionFollowedByNone),
            _ => ConjunctionFollowedBy::Custom(s),
        })
    }
}

impl ToSql for ConjunctionFollowedBy {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(ValueRef::Text(
            self.as_ref().as_bytes(),
        )))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum Transitivity {
    Transitive,
    Intransitive,
    Ambitransitive,
}

impl Transitivity {
    pub fn explicit_moderation_page(&self) -> &str {
        match self {
            Transitivity::Transitive => "transitive-only",
            Transitivity::Intransitive => "intransitive",
            Transitivity::Ambitransitive => "ambitransitive",
        }
    }

    pub fn explicit_word_details_page(&self) -> &str {
        match self {
            Transitivity::Transitive => "transitive-only",
            Transitivity::Intransitive => "intransitive",
            Transitivity::Ambitransitive => "either",
        }
    }
}

impl AsRef<str> for Transitivity {
    fn as_ref(&self) -> &str {
        self.explicit_word_details_page()
    }
}

impl Display for Transitivity {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            Transitivity::Transitive => "transitive-only",
            Transitivity::Intransitive => "intransitive",
            Transitivity::Ambitransitive => "",
        };

        f.write_str(s)
    }
}

pub struct InvalidTransitivity(String);

impl Display for InvalidTransitivity {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("invalid transitivity: `{}`", self.0))
    }
}

impl FromStr for Transitivity {
    type Err = InvalidTransitivity;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &s.trim().to_lowercase()[..] {
            "transitive-only" | "transitive" => Ok(Transitivity::Transitive),
            "intransitive" => Ok(Transitivity::Intransitive),
            "ambitransitive" | "either" => Ok(Transitivity::Ambitransitive),
            _ => Err(InvalidTransitivity(s.to_owned())),
        }
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
    fn as_u8(&self) -> u8;
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
    fn as_u8(&self) -> u8 {
        *self as u8
    }
}

#[derive(
    IntoPrimitive,
    TryFromPrimitive,
    Serialize,
    Deserialize,
    Copy,
    Clone,
    Debug,
    PartialOrd,
    Ord,
    PartialEq,
    Eq,
    EnumString,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum WordLinkType {
    PluralOrSingular = 1,
    AlternateUse = 2,
    Antonym = 3,
    Related = 4,
    Confusable = 5,
}

impl WordLinkType {
    fn as_str(&self) -> &'static str {
        match self {
            WordLinkType::PluralOrSingular => "Plural or singular form",
            WordLinkType::Antonym => "Antonym",
            WordLinkType::Related => "Related meaning",
            WordLinkType::Confusable => "Confusable",
            WordLinkType::AlternateUse => "Alternate use",
        }
    }

    pub fn as_u8(&self) -> u8 {
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
        write!(f, "{}", self.as_str())
    }
}
