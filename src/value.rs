use crate::{Error, Map, Number};
use serde::de::{DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::mem;
use std::ops::{Index, IndexMut};

/// An owned JSON value.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map<String, Value>),
}

impl Value {
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    #[must_use]
    pub const fn is_boolean(&self) -> bool {
        matches!(self, Self::Bool(_))
    }

    #[must_use]
    pub const fn is_number(&self) -> bool {
        matches!(self, Self::Number(_))
    }

    #[must_use]
    pub fn is_i64(&self) -> bool {
        self.as_i64().is_some()
    }

    #[must_use]
    pub fn is_u64(&self) -> bool {
        self.as_u64().is_some()
    }

    #[must_use]
    pub fn is_f64(&self) -> bool {
        matches!(self, Self::Number(number) if number.is_f64())
    }

    #[must_use]
    pub const fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    #[must_use]
    pub const fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    #[must_use]
    pub const fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(value) => value.as_i64(),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Number(value) => value.as_u64(),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => value.as_f64(),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_number(&self) -> Option<&Number> {
        match self {
            Self::Number(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_array(&self) -> Option<&Vec<Self>> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub const fn as_array_mut(&mut self) -> Option<&mut Vec<Self>> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_object(&self) -> Option<&Map<String, Self>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub const fn as_object_mut(&mut self) -> Option<&mut Map<String, Self>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub fn get<I: ValueIndex>(&self, index: I) -> Option<&Self> {
        index.index_into(self)
    }

    pub fn get_mut<I: ValueIndex>(&mut self, index: I) -> Option<&mut Self> {
        index.index_into_mut(self)
    }

    /// Resolves an RFC 6901 JSON Pointer.
    #[must_use]
    pub fn pointer(&self, pointer: &str) -> Option<&Self> {
        if pointer.is_empty() {
            return Some(self);
        }
        if !pointer.starts_with('/') {
            return None;
        }
        pointer.split('/').skip(1).try_fold(self, |value, token| {
            let token = decode_pointer_token(token);
            match value {
                Self::Object(object) => object.get(token.as_ref()),
                Self::Array(array) => array.get(parse_index(&token)?),
                _ => None,
            }
        })
    }

    /// Resolves a mutable RFC 6901 JSON Pointer.
    pub fn pointer_mut(&mut self, pointer: &str) -> Option<&mut Self> {
        if pointer.is_empty() {
            return Some(self);
        }
        if !pointer.starts_with('/') {
            return None;
        }
        let tokens = pointer
            .split('/')
            .skip(1)
            .map(decode_pointer_token)
            .collect::<Vec<_>>();
        let mut current = self;
        for token in tokens {
            current = match current {
                Self::Object(object) => object.get_mut(token.as_ref())?,
                Self::Array(array) => array.get_mut(parse_index(&token)?)?,
                _ => return None,
            };
        }
        Some(current)
    }

    /// Replaces this value with null and returns the previous value.
    #[must_use]
    pub fn take(&mut self) -> Self {
        mem::take(self)
    }
}

fn decode_pointer_token(token: &str) -> std::borrow::Cow<'_, str> {
    if !token.as_bytes().contains(&b'~') {
        return std::borrow::Cow::Borrowed(token);
    }
    std::borrow::Cow::Owned(token.replace("~1", "/").replace("~0", "~"))
}

fn parse_index(token: &str) -> Option<usize> {
    if token.is_empty() || (token.len() > 1 && token.starts_with('0')) {
        return None;
    }
    token.parse().ok()
}

/// Index accepted by [`Value::get`] and [`Value::get_mut`].
pub trait ValueIndex: private::Sealed {
    fn index_into(self, value: &Value) -> Option<&Value>;
    fn index_into_mut(self, value: &mut Value) -> Option<&mut Value>;
}

mod private {
    pub trait Sealed {}
    impl Sealed for usize {}
    impl Sealed for &str {}
    impl Sealed for String {}
    impl Sealed for &String {}
}

impl ValueIndex for usize {
    fn index_into(self, value: &Value) -> Option<&Value> {
        value.as_array()?.get(self)
    }

    fn index_into_mut(self, value: &mut Value) -> Option<&mut Value> {
        value.as_array_mut()?.get_mut(self)
    }
}

impl ValueIndex for &str {
    fn index_into(self, value: &Value) -> Option<&Value> {
        value.as_object()?.get(self)
    }

    fn index_into_mut(self, value: &mut Value) -> Option<&mut Value> {
        value.as_object_mut()?.get_mut(self)
    }
}

impl ValueIndex for String {
    fn index_into(self, value: &Value) -> Option<&Value> {
        value.as_object()?.get(&self)
    }

    fn index_into_mut(self, value: &mut Value) -> Option<&mut Value> {
        value.as_object_mut()?.get_mut(&self)
    }
}

impl ValueIndex for &String {
    fn index_into(self, value: &Value) -> Option<&Value> {
        value.as_object()?.get(self)
    }

    fn index_into_mut(self, value: &mut Value) -> Option<&mut Value> {
        value.as_object_mut()?.get_mut(self)
    }
}

static NULL: Value = Value::Null;

impl Index<&str> for Value {
    type Output = Self;

    fn index(&self, index: &str) -> &Self::Output {
        self.get(index).unwrap_or(&NULL)
    }
}

impl IndexMut<&str> for Value {
    fn index_mut(&mut self, index: &str) -> &mut Self::Output {
        if self.is_null() {
            *self = Self::Object(Map::new());
        }
        self.as_object_mut()
            .expect("cannot access a non-object JSON value with a string")
            .entry(index.to_owned())
            .or_default()
    }
}

impl Index<usize> for Value {
    type Output = Self;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).unwrap_or(&NULL)
    }
}

impl IndexMut<usize> for Value {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self
            .as_array_mut()
            .expect("cannot access a non-array JSON value with an integer")[index]
    }
}

impl Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Number(value) => value.serialize(serializer),
            Self::String(value) => serializer.serialize_str(value),
            Self::Array(value) => value.serialize(serializer),
            Self::Object(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("any valid JSON value")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(Value::Null)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(Value::Null)
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(Value::Bool(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(Value::from(value))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Value::from(value))
            }

            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Number::from_f64(value)
                    .map(Value::Number)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Value::String(value.to_owned()))
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E> {
                Ok(Value::String(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(Value::String(value))
            }

            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0));
                while let Some(value) = sequence.next_element()? {
                    values.push(value);
                }
                Ok(Value::Array(values))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some((key, value)) = map.next_entry()? {
                    values.insert(key, value);
                }
                Ok(Value::Object(values))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match crate::to_string(self) {
            Ok(encoded) => formatter.write_str(&encoded),
            Err(_) => Err(fmt::Error),
        }
    }
}

impl From<()> for Value {
    fn from((): ()) -> Self {
        Self::Null
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<Number> for Value {
    fn from(value: Number) -> Self {
        Self::Number(value)
    }
}

macro_rules! from_number {
    ($($type:ty),+ $(,)?) => {
        $(
            impl From<$type> for Value {
                fn from(value: $type) -> Self {
                    Self::Number(Number::from(value))
                }
            }
        )+
    };
}

from_number!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Number::from_f64(f64::from(value)).map_or(Self::Null, Self::Number)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Number::from_f64(value).map_or(Self::Null, Self::Number)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(value: Vec<T>) -> Self {
        Self::Array(value.into_iter().map(Into::into).collect())
    }
}

macro_rules! partial_eq_value {
    ($($type:ty),+ $(,)?) => {
        $(
            impl PartialEq<$type> for Value {
                fn eq(&self, other: &$type) -> bool {
                    self == &Value::from(*other)
                }
            }

            impl PartialEq<Value> for $type {
                fn eq(&self, other: &Value) -> bool {
                    &Value::from(*self) == other
                }
            }
        )+
    };
}

partial_eq_value!(
    bool, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64
);

impl PartialEq<str> for Value {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == Some(other)
    }
}

impl PartialEq<&str> for Value {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

impl PartialEq<Value> for str {
    fn eq(&self, other: &Value) -> bool {
        other == self
    }
}

impl PartialEq<Value> for &str {
    fn eq(&self, other: &Value) -> bool {
        other == *self
    }
}

/// Deserializes a typed value directly from an owned [`Value`].
///
/// # Errors
///
/// Returns an error when the value does not match `T`.
pub fn from_value<T: DeserializeOwned>(value: Value) -> Result<T, Error> {
    T::deserialize(value)
}
