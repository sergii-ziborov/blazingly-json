use crate::{from_str, Result};
use serde::de::{Deserializer, Visitor};
use serde::Deserialize;
use std::fmt;
use std::marker::PhantomData;

pub(crate) const RAW_JSON_TOKEN: &str = "$blazingly_json::RawJson";

/// A validated JSON value borrowed directly from the input buffer.
///
/// `RawJson` is useful for protocol envelopes that need to inspect a few
/// fields while deferring or avoiding deserialization of a large payload.
/// Capturing it performs no allocation and preserves the exact compact or
/// pretty representation from the input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawJson<'a> {
    json: &'a str,
}

impl<'a> RawJson<'a> {
    pub(crate) const fn new(json: &'a str) -> Self {
        Self { json }
    }

    /// Returns the original JSON representation of this value.
    #[must_use]
    pub const fn get(self) -> &'a str {
        self.json
    }

    /// Deserializes the captured value only when the caller needs it.
    ///
    /// # Errors
    ///
    /// Returns an error when the JSON does not match `T`.
    pub fn deserialize<T>(self) -> Result<T>
    where
        T: Deserialize<'a>,
    {
        from_str(self.json)
    }
}

struct RawJsonVisitor<'a>(PhantomData<&'a str>);

impl<'de: 'a, 'a> Visitor<'de> for RawJsonVisitor<'a> {
    type Value = RawJson<'a>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a borrowed raw JSON value")
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> std::result::Result<Self::Value, E> {
        Ok(RawJson::new(value))
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for RawJson<'a> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct(RAW_JSON_TOKEN, RawJsonVisitor(PhantomData))
    }
}
