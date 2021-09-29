use askama_warp::warp::http::header::CONTENT_TYPE;
use num_enum::TryFromPrimitive;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::Row;
use serde::de::{DeserializeOwned};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::{TryFrom, TryInto};
use std::error::Error as StdError;
use std::fmt::{self, Debug, Display, Formatter};
use warp::hyper::body::Bytes;
use warp::{Buf, Filter, Rejection};

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

impl StdError for DiscrimOutOfRange {}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct SerOnlyDisplay<T>(pub T);

impl<T: Display> Display for SerOnlyDisplay<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Display> Serialize for SerOnlyDisplay<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}", self.0))
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for SerOnlyDisplay<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(SerOnlyDisplay)
    }
}

#[derive(Debug)]
pub enum WithDeleteSentinel<T> {
    Some(T),
    Remove,
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

pub fn false_fn() -> bool {
    false
}

pub fn deserialize_checkbox<'de, D>(deser: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match String::deserialize(deser)? {
        str if str.to_lowercase() == "on" || str.to_lowercase() == "true" => Ok(true),
        str if str.to_lowercase() == "off" || str.to_lowercase() == "false" => Ok(false),
        other => Err(serde::de::Error::custom(format!(
            "Invalid checkbox bool string {}",
            other
        ))),
    }
}

fn to_bytes<B: Buf>(mut b: B) -> Bytes {
    b.copy_to_bytes(b.remaining())
}

pub fn qs_form<T: DeserializeOwned + Send>() -> impl Filter<Extract = (T,), Error = Rejection> + Copy
{
    warp::header::exact(CONTENT_TYPE.as_ref(), "application/x-www-form-urlencoded")
        .and(warp::body::aggregate())
        .map(to_bytes)
        .and_then(|bytes: Bytes| async move {
            serde_qs::Config::new(5, false)
                .deserialize_bytes(&bytes)
                .map_err(|err| {
                    #[derive(Debug)]
                    struct DeserErr(serde_qs::Error);

                    log::debug!("Error deserializing query-string: {:?}", err);

                    impl warp::reject::Reject for DeserErr {}

                    warp::reject::custom(DeserErr(err))
                })
        })
}
