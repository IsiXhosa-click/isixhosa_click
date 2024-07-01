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

#[derive(Debug)]
pub enum WithDeleteSentinel<T> {
    Some(T),
    Remove,
}
