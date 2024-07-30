use crate::format::{DisplayHtml, HtmlFormatter};
use crate::i18n::{ToTranslationKey, TranslationKey};
use fluent_templates::Loader;
use isixhosa::noun::NounClass;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::{self, Debug, Display, Formatter};
use std::str::FromStr;

#[derive(
    IntoPrimitive,
    TryFromPrimitive,
    Serialize,
    Deserialize,
    Copy,
    Clone,
    Debug,
    Hash,
    PartialEq,
    Eq,
    Ord,
    PartialOrd,
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
    Ideophone = 9,
    BoundMorpheme = 10,
}

impl PartOfSpeech {
    /// Used in askama templates
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

impl PartOfSpeech {
    pub fn name(&self) -> String {
        match self {
            PartOfSpeech::BoundMorpheme => "bound_morpheme".to_owned(),
            _ => format!("{:?}", self).to_lowercase(),
        }
    }
}

impl ToTranslationKey for PartOfSpeech {
    // Ensure consistency with the serde serialization
    fn translation_key(&self) -> TranslationKey<'_> {
        let s = match self {
            PartOfSpeech::BoundMorpheme => "bound_morpheme".to_owned(),
            _ => format!("{:?}", self).to_lowercase(),
        };

        TranslationKey(Cow::Owned(s))
    }
}

impl<L: Loader + 'static> DisplayHtml<L> for PartOfSpeech {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        f.write_text(&self.translation_key())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

impl<L: Loader + 'static> DisplayHtml<L> for ConjunctionFollowedBy {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        match self {
            ConjunctionFollowedBy::Indicative => {
                f.write_text(&TranslationKey::new("followed-by.indicative"))
            }
            ConjunctionFollowedBy::Subjunctive => {
                f.write_text(&TranslationKey::new("followed-by.subjunctive"))
            }
            ConjunctionFollowedBy::Participial => {
                f.write_text(&TranslationKey::new("followed-by.participial"))
            }
            ConjunctionFollowedBy::Custom(s) => f.write_raw_str(s),
        }
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

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive, Serialize, Deserialize,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum Transitivity {
    Transitive,
    Intransitive,
    Ambitransitive,
}

impl Transitivity {
    pub fn name(&self) -> &'static str {
        match self {
            Transitivity::Transitive => "transitive",
            Transitivity::Intransitive => "intransitive",
            Transitivity::Ambitransitive => "ambitransitive",
        }
    }
}

/// This is used to serialize in WordHit
impl ToTranslationKey for Transitivity {
    fn translation_key(&self) -> TranslationKey<'_> {
        match self {
            Transitivity::Transitive => TranslationKey::new("transitive"),
            Transitivity::Intransitive => TranslationKey::new("intransitive"),
            Transitivity::Ambitransitive => TranslationKey::new("ambitransitive.in-word-result"),
        }
    }
}

impl Transitivity {
    pub fn explicit_moderation_page(&self) -> TranslationKey<'static> {
        match self {
            Transitivity::Transitive => TranslationKey::new("transitive.explicit"),
            Transitivity::Intransitive => TranslationKey::new("intransitive.explicit"),
            Transitivity::Ambitransitive => TranslationKey::new("ambitransitive.explicit"),
        }
    }

    pub fn explicit_word_details_page(&self) -> TranslationKey<'static> {
        match self {
            Transitivity::Transitive => TranslationKey::new("transitive"),
            Transitivity::Intransitive => TranslationKey::new("intransitive"),
            Transitivity::Ambitransitive => TranslationKey::new("ambitransitive"),
        }
    }
}

impl<L: Loader + 'static> DisplayHtml<L> for Transitivity {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        let s = match self {
            Transitivity::Transitive => TranslationKey::new("transitive.in-word-result"),
            Transitivity::Intransitive => TranslationKey::new("intransitive.in-word-result"),
            Transitivity::Ambitransitive => TranslationKey::new("ambitransitive.in-word-result"),
        };

        f.write_text(&s)
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

/// Noun class prefixes with singular and plural
#[derive(Clone, Debug, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct NounClassPrefixes {
    pub selected_singular: bool,
    pub singular: Cow<'static, str>,
    pub plural: Option<Cow<'static, str>>,
}

impl NounClassPrefixes {
    fn from_singular_plural(
        selected_singular: bool,
        singular: &'static str,
        plural: &'static str,
    ) -> Self {
        NounClassPrefixes {
            selected_singular,
            singular: Cow::Borrowed(singular),
            plural: Some(Cow::Borrowed(plural)),
        }
    }

    fn singular_class(singular: &'static str) -> Self {
        NounClassPrefixes {
            selected_singular: true,
            singular: Cow::Borrowed(singular),
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
            Class1Um | Aba => both(*self == Class1Um, "um", "aba"),
            U | Oo => both(*self == U, "u", "oo"),
            Class3Um | Imi => both(*self == Class3Um, "um", "imi"),
            Ili | Ama => both(*self == Ili, "i(li)", "ama"),
            Isi | Izi => both(*self == Isi, "isi", "izi"),
            In | Izin => both(*self == In, "i(n)", "i(z)in"),
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

#[allow(dead_code)] // In case we want to use field this later
pub struct InvalidWordLinkType(String);

impl FromStr for WordLinkType {
    type Err = InvalidWordLinkType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "plural_or_singular" => WordLinkType::PluralOrSingular,
            "alternate_use" => WordLinkType::AlternateUse,
            "antonym" => WordLinkType::Antonym,
            "related" => WordLinkType::Related,
            "confusable" => WordLinkType::Confusable,
            _ => return Err(InvalidWordLinkType(s.to_owned())),
        })
    }
}

impl<L: Loader + 'static> DisplayHtml<L> for WordLinkType {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        let s = match self {
            WordLinkType::PluralOrSingular => TranslationKey::new("linked-words.plurality"),
            WordLinkType::Antonym => TranslationKey::new("linked-words.antonym"),
            WordLinkType::Related => TranslationKey::new("linked-words.related"),
            WordLinkType::Confusable => TranslationKey::new("linked-words.confusable"),
            WordLinkType::AlternateUse => TranslationKey::new("linked-words.alternate"),
        };

        f.write_text(&s)
    }
}
