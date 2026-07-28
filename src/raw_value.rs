#![allow(unsafe_code)]

//! The only unsafe code in `blazingly-json`.
//!
//! `RawValue` is a transparent dynamically sized wrapper around `str`. The
//! three conversions below preserve the data pointer and slice metadata
//! exactly. No bytes are read through a different layout, and owned
//! conversions preserve the original allocation and allocator.

use crate::{from_str, to_string, Error, Result};
use serde::de::{self, DeserializeSeed, MapAccess, Unexpected, Visitor};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{self, Write};
use std::mem;

/// Serde's established private representation for a verbatim JSON value.
///
/// Using the same token makes this type interoperable with `serde_json`
/// serializers and deserializers that support `RawValue`.
pub(crate) const RAW_VALUE_TOKEN: &str = "$serde_json::private::RawValue";

/// A reference to one complete, validated JSON value.
///
/// Borrowed values point directly into the input buffer. Serializing a
/// `RawValue` writes its original representation verbatim, including internal
/// whitespace.
#[repr(transparent)]
pub struct RawValue {
    json: str,
}

impl RawValue {
    /// A raw JSON `null`.
    pub const NULL: &'static Self = Self::from_borrowed("null");

    /// A raw JSON `true`.
    pub const TRUE: &'static Self = Self::from_borrowed("true");

    /// A raw JSON `false`.
    pub const FALSE: &'static Self = Self::from_borrowed("false");

    #[inline]
    const fn from_borrowed(json: &str) -> &Self {
        // SAFETY: RawValue is repr(transparent) over its only field, `str`.
        // Therefore &str and &RawValue have identical data and metadata.
        unsafe { mem::transmute::<&str, &Self>(json) }
    }

    #[inline]
    fn from_owned(json: Box<str>) -> Box<Self> {
        // SAFETY: RawValue is repr(transparent) over `str`. Box preserves the
        // same allocation, length metadata, alignment, and allocator.
        unsafe { mem::transmute::<Box<str>, Box<Self>>(json) }
    }

    #[inline]
    fn into_owned(raw: Box<Self>) -> Box<str> {
        // SAFETY: This is the exact inverse of from_owned and restores the
        // original Box<str> representation and deallocation layout.
        unsafe { mem::transmute::<Box<Self>, Box<str>>(raw) }
    }

    /// Validates an owned JSON string and converts it into a boxed raw value.
    ///
    /// When the input has no surrounding whitespace this reuses its allocation.
    /// Otherwise only the validated JSON value is copied.
    ///
    /// # Errors
    ///
    /// Returns an error unless the string contains exactly one valid JSON
    /// value, optionally surrounded by whitespace.
    pub fn from_string(json: String) -> Result<Box<Self>> {
        let borrowed = from_str::<&Self>(&json)?;
        if borrowed.get().len() != json.len() {
            return Ok(borrowed.to_owned());
        }
        Ok(Self::from_owned(json.into_boxed_str()))
    }

    /// Converts an owned JSON string without validating it.
    ///
    /// # Safety
    ///
    /// `json` must contain exactly one well-formed JSON value with no leading
    /// or trailing whitespace. Raw values are emitted verbatim, so violating
    /// this invariant can make subsequent serializer output invalid.
    #[must_use]
    pub unsafe fn from_string_unchecked(json: String) -> Box<Self> {
        debug_assert!(
            from_str::<&Self>(&json).is_ok_and(|raw| raw.get().len() == json.len()),
            "RawValue::from_string_unchecked requires one valid JSON value without surrounding whitespace",
        );
        Self::from_owned(json.into_boxed_str())
    }

    /// Returns the exact JSON representation.
    #[must_use]
    #[inline]
    pub fn get(&self) -> &str {
        &self.json
    }

    /// Copies the raw JSON bytes directly into an exactly sized vector.
    ///
    /// This bypasses the generic Serde marker protocol when the raw value is
    /// the complete output document.
    #[must_use]
    #[inline]
    pub fn to_vec(&self) -> Vec<u8> {
        self.json.as_bytes().to_vec()
    }

    /// Writes the raw JSON bytes directly to a writer.
    ///
    /// Use generic serialization instead when embedding this value inside a
    /// larger structure.
    ///
    /// # Errors
    ///
    /// Returns any error produced by `writer`.
    #[inline]
    pub fn write_to<W: Write>(&self, mut writer: W) -> io::Result<()> {
        writer.write_all(self.json.as_bytes())
    }

    /// Converts a boxed raw value back into a string while reusing its
    /// allocation.
    #[must_use]
    #[inline]
    pub fn into_string(self: Box<Self>) -> String {
        String::from(Self::into_owned(self))
    }
}

impl Clone for Box<RawValue> {
    #[inline]
    fn clone(&self) -> Self {
        (**self).to_owned()
    }
}

impl ToOwned for RawValue {
    type Owned = Box<Self>;

    #[inline]
    fn to_owned(&self) -> Self::Owned {
        RawValue::from_owned(self.json.to_owned().into_boxed_str())
    }
}

impl Default for Box<RawValue> {
    #[inline]
    fn default() -> Self {
        RawValue::NULL.to_owned()
    }
}

impl fmt::Debug for RawValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("RawValue")
            .field(&format_args!("{}", &self.json))
            .finish()
    }
}

impl fmt::Display for RawValue {
    #[inline]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.json)
    }
}

impl From<Box<RawValue>> for Box<str> {
    #[inline]
    fn from(raw: Box<RawValue>) -> Self {
        RawValue::into_owned(raw)
    }
}

/// Serializes a value once and retains the resulting JSON verbatim.
///
/// # Errors
///
/// Returns an error if `value` cannot be represented as JSON.
pub fn to_raw_value<T>(value: &T) -> Result<Box<RawValue>>
where
    T: Serialize + ?Sized,
{
    let json = to_string(value)?;
    Ok(RawValue::from_owned(json.into_boxed_str()))
}

impl Serialize for RawValue {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut raw = serializer.serialize_struct(RAW_VALUE_TOKEN, 1)?;
        raw.serialize_field(RAW_VALUE_TOKEN, &self.json)?;
        raw.end()
    }
}

struct RawKey;

impl<'de> Deserialize<'de> for RawKey {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct RawKeyVisitor;

        impl Visitor<'_> for RawKeyVisitor {
            type Value = ();

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("the RawValue marker")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<(), E>
            where
                E: de::Error,
            {
                if value == RAW_VALUE_TOKEN {
                    Ok(())
                } else {
                    Err(E::custom("unexpected raw value marker"))
                }
            }
        }

        deserializer.deserialize_identifier(RawKeyVisitor)?;
        Ok(Self)
    }
}

struct BorrowedRawSeed;

impl<'de> DeserializeSeed<'de> for BorrowedRawSeed {
    type Value = &'de RawValue;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for BorrowedRawSeed {
    type Value = &'de RawValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a borrowed raw JSON value")
    }

    #[inline]
    fn visit_borrowed_str<E>(self, value: &'de str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(RawValue::from_borrowed(value))
    }
}

struct BoxedRawSeed;

impl<'de> DeserializeSeed<'de> for BoxedRawSeed {
    type Value = Box<RawValue>;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_str(self)
    }
}

impl Visitor<'_> for BoxedRawSeed {
    type Value = Box<RawValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an owned raw JSON value")
    }

    #[inline]
    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(RawValue::from_owned(value.to_owned().into_boxed_str()))
    }

    #[inline]
    fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(RawValue::from_owned(value.into_boxed_str()))
    }
}

struct BorrowedRawVisitor;

impl<'de> Visitor<'de> for BorrowedRawVisitor {
    type Value = &'de RawValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any valid JSON value")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        if map.next_key::<RawKey>()?.is_none() {
            return Err(de::Error::invalid_type(Unexpected::Map, &self));
        }
        map.next_value_seed(BorrowedRawSeed)
    }
}

struct BoxedRawVisitor;

impl<'de> Visitor<'de> for BoxedRawVisitor {
    type Value = Box<RawValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any valid JSON value")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        if map.next_key::<RawKey>()?.is_none() {
            return Err(de::Error::invalid_type(Unexpected::Map, &self));
        }
        map.next_value_seed(BoxedRawSeed)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for &'a RawValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct(RAW_VALUE_TOKEN, BorrowedRawVisitor)
    }
}

impl<'de> Deserialize<'de> for Box<RawValue> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct(RAW_VALUE_TOKEN, BoxedRawVisitor)
    }
}

impl<'de> de::IntoDeserializer<'de, Error> for &'de RawValue {
    type Deserializer = &'de RawValue;

    #[inline]
    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

macro_rules! forward_raw_visitor {
    ($($method:ident),+ $(,)?) => {
        $(
            fn $method<V>(self, visitor: V) -> Result<V::Value>
            where
                V: Visitor<'de>,
            {
                let mut deserializer = crate::Deserializer::from_str(self.get());
                de::Deserializer::$method(&mut deserializer, visitor)
            }
        )+
    };
}

impl<'de> de::Deserializer<'de> for &'de RawValue {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut deserializer = crate::Deserializer::from_str(self.get());
        de::Deserializer::deserialize_any(&mut deserializer, visitor)
    }

    forward_raw_visitor! {
        deserialize_bool,
        deserialize_i8,
        deserialize_i16,
        deserialize_i32,
        deserialize_i64,
        deserialize_i128,
        deserialize_u8,
        deserialize_u16,
        deserialize_u32,
        deserialize_u64,
        deserialize_u128,
        deserialize_f32,
        deserialize_f64,
        deserialize_char,
        deserialize_str,
        deserialize_string,
        deserialize_bytes,
        deserialize_byte_buf,
        deserialize_option,
        deserialize_unit,
        deserialize_seq,
        deserialize_map,
        deserialize_identifier,
        deserialize_ignored_any
    }

    fn deserialize_unit_struct<V>(self, name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut deserializer = crate::Deserializer::from_str(self.get());
        de::Deserializer::deserialize_unit_struct(&mut deserializer, name, visitor)
    }

    fn deserialize_newtype_struct<V>(self, name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut deserializer = crate::Deserializer::from_str(self.get());
        de::Deserializer::deserialize_newtype_struct(&mut deserializer, name, visitor)
    }

    fn deserialize_tuple<V>(self, length: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut deserializer = crate::Deserializer::from_str(self.get());
        de::Deserializer::deserialize_tuple(&mut deserializer, length, visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        length: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut deserializer = crate::Deserializer::from_str(self.get());
        de::Deserializer::deserialize_tuple_struct(&mut deserializer, name, length, visitor)
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut deserializer = crate::Deserializer::from_str(self.get());
        de::Deserializer::deserialize_struct(&mut deserializer, name, fields, visitor)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let mut deserializer = crate::Deserializer::from_str(self.get());
        de::Deserializer::deserialize_enum(&mut deserializer, name, variants, visitor)
    }
}
