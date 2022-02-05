use askama_warp::warp::http::header::CONTENT_TYPE;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use serde::{Deserialize, Deserializer};
use tracing::warn;
use warp::hyper::body::Bytes;
use warp::{Buf, Filter, Rejection};

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

                    warn!("Error deserializing query-string: {:?}", err);

                    impl warp::reject::Reject for DeserErr {}

                    warp::reject::custom(DeserErr(err))
                })
        })
}
