use isixhosa::noun::NounClass;
use num_enum::TryFromPrimitive;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryInto;
use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};
use std::marker::PhantomData;

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
pub struct SerializeDisplay<T>(pub T);

impl<T: Display> Display for SerializeDisplay<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Display> Serialize for SerializeDisplay<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}", self.0))
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for SerializeDisplay<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(SerializeDisplay)
    }
}

pub trait OptionMapNounClassExt {
    fn map_noun_class(&self) -> Option<NounClass>;
}

impl<P> OptionMapNounClassExt for Option<SerializePrimitive<NounClass, P>> {
    fn map_noun_class(&self) -> Option<NounClass> {
        self.as_ref().map(|s| s.val)
    }
}

impl OptionMapNounClassExt for Option<NounClass> {
    fn map_noun_class(&self) -> Option<NounClass> {
        *self
    }
}

#[derive(Copy, Clone, Hash, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SerializePrimitive<T, P> {
    pub val: T,
    phantom: PhantomData<fn() -> P>,
}

impl<T, P> SerializePrimitive<T, P> {
    pub fn new(val: T) -> Self {
        SerializePrimitive {
            val,
            phantom: PhantomData,
        }
    }
}

impl<T, P> Serialize for SerializePrimitive<T, P>
where
    T: Into<P> + Copy,
    P: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.val.into().serialize(serializer)
    }
}

impl<'de, T, P> Deserialize<'de> for SerializePrimitive<T, P>
where
    T: TryFromPrimitive<Primitive = P> + Copy,
    P: Deserialize<'de> + Copy + Into<i64>,
{
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let primitive = P::deserialize(de)?;
        let discrim = primitive.into();
        T::try_from_primitive(primitive)
            .map(SerializePrimitive::new)
            .map_err(|_| D::Error::custom(DiscrimOutOfRange(discrim, std::any::type_name::<T>())))
    }
}

pub enum NounClassOpt {
    Some(NounClass),
    Remove,
}

pub trait NounClassOptExt {
    fn flatten(self) -> Option<NounClass>;
}

impl NounClassOptExt for Option<NounClassOpt> {
    fn flatten(self) -> Option<NounClass> {
        self.and_then(|x| match x {
            NounClassOpt::Some(v) => Some(v),
            NounClassOpt::Remove => None,
        })
    }
}

impl FromSql for NounClassOpt {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;

        if v == 255 {
            Ok(NounClassOpt::Remove)
        } else {
            let err = || FromSqlError::Other(Box::new(DiscrimOutOfRange(v, "NounClass")));
            NounClass::try_from_primitive(v.try_into().map_err(|_| err())?)
                .map_err(|_| err())
                .map(NounClassOpt::Some)
        }
    }
}
