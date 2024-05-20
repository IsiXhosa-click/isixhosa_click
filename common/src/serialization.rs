use crate::i18n::{translate, I18nInfo, ToTranslationKey};
use serde::{Serialize, Serializer};
use std::cmp::Ordering;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone)]
pub struct DiscrimOutOfRange(pub i64, pub &'static str);

impl Display for DiscrimOutOfRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "discriminator {} out of range for type {}",
            self.0, self.1
        )
    }
}

impl Error for DiscrimOutOfRange {}

#[derive(Clone, Debug)]
pub struct SerializeTranslated<T> {
    pub val: T,
    pub i18n_info: I18nInfo,
}

impl<T: PartialEq> PartialEq<Self> for SerializeTranslated<T> {
    fn eq(&self, other: &SerializeTranslated<T>) -> bool {
        self.val.eq(&other.val)
    }
}

impl<T: PartialOrd + PartialEq> PartialOrd<Self> for SerializeTranslated<T> {
    fn partial_cmp(&self, other: &SerializeTranslated<T>) -> Option<Ordering> {
        self.val.partial_cmp(&other.val)
    }
}

impl<T: Ord> Eq for SerializeTranslated<T> {}

impl<T: Ord> Ord for SerializeTranslated<T> {
    fn cmp(&self, other: &SerializeTranslated<T>) -> Ordering {
        self.val.cmp(&other.val)
    }
}

impl<T: PartialEq> PartialEq<T> for SerializeTranslated<T> {
    fn eq(&self, other: &T) -> bool {
        &self.val == other
    }
}

// impl<T: DisplayHtml> Display for SerAndDisplayWithDisplayHtml<T> {
//     fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
//         f.write_str(&self.0.to_plaintext().to_string())
//     }
// }

impl<T: ToTranslationKey> Serialize for SerializeTranslated<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&translate(
            &self.val.translation_key(),
            &self.i18n_info,
            &Default::default(),
        ))
    }
}

// impl<'de, T: Deserialize<'de>> Deserialize<'de> for SerializeTranslated<T> {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         T::deserialize(deserializer).map(SerializeTranslated)
//     }
// }

// impl<T: FromStr> FromStr for SerializeTranslated<T> {
//     type Err = T::Err;
//
//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         T::from_str(s).map(SerializeTranslated)
//     }
// }

#[derive(Debug)]
pub enum WithDeleteSentinel<T> {
    Some(T),
    Remove,
}
