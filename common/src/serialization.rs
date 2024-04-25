use crate::format::DisplayHtml;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

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

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct SerAndDisplayWithDisplayHtml<T>(pub T);

impl<T: PartialEq> PartialEq<T> for SerAndDisplayWithDisplayHtml<T> {
    fn eq(&self, other: &T) -> bool {
        &self.0 == other
    }
}

impl<T: DisplayHtml> Display for SerAndDisplayWithDisplayHtml<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_plaintext().to_string())
    }
}

impl<T: DisplayHtml> Serialize for SerAndDisplayWithDisplayHtml<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_plaintext().to_string())
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for SerAndDisplayWithDisplayHtml<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(SerAndDisplayWithDisplayHtml)
    }
}

impl<T: FromStr> FromStr for SerAndDisplayWithDisplayHtml<T> {
    type Err = T::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        T::from_str(s).map(SerAndDisplayWithDisplayHtml)
    }
}

#[derive(Debug)]
pub enum WithDeleteSentinel<T> {
    Some(T),
    Remove,
}
