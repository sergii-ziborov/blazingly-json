use crate::{Error, Result, Value};
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use std::collections::btree_map;
use std::vec;

impl<'de> de::Deserializer<'de> for Value {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self {
            Self::Null => visitor.visit_unit(),
            Self::Bool(value) => visitor.visit_bool(value),
            Self::Number(value) if value.is_u64() => visitor.visit_u64(
                value
                    .as_u64()
                    .expect("a u64 JSON number must contain a u64"),
            ),
            Self::Number(value) if value.is_i64() => visitor.visit_i64(
                value
                    .as_i64()
                    .expect("an i64 JSON number must contain an i64"),
            ),
            Self::Number(value) => visitor.visit_f64(
                value
                    .as_f64()
                    .expect("a JSON number must be representable as f64"),
            ),
            Self::String(value) => visitor.visit_string(value),
            Self::Array(values) => visitor.visit_seq(ValueSeqAccess {
                values: values.into_iter(),
            }),
            Self::Object(values) => visitor.visit_map(ValueMapAccess {
                values: values.into_iter(),
                pending: None,
            }),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self {
            Self::Bool(value) => visitor.visit_bool(value),
            other => Err(type_error("boolean", &other)),
        }
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i8(integer(&self)?)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i16(integer(&self)?)
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i32(integer(&self)?)
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i64(integer(&self)?)
    }

    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i128(integer(&self)?)
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u8(unsigned(&self)?)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u16(unsigned(&self)?)
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u32(unsigned(&self)?)
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u64(unsigned(&self)?)
    }

    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u128(unsigned(&self)?)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        #[allow(clippy::cast_possible_truncation)]
        let value = float(&self)? as f32;
        visitor.visit_f32(value)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_f64(float(&self)?)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self {
            Self::String(value) => {
                let mut characters = value.chars();
                let character = characters
                    .next()
                    .ok_or_else(|| Error::message("expected one character"))?;
                if characters.next().is_some() {
                    return Err(Error::message("expected one character"));
                }
                visitor.visit_char(character)
            }
            other => Err(type_error("string", &other)),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self {
            Self::String(value) => visitor.visit_string(value),
            other => Err(type_error("string", &other)),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if self.is_null() {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if self.is_null() {
            visitor.visit_unit()
        } else {
            Err(type_error("null", &self))
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self {
            Self::Array(values) => visitor.visit_seq(ValueSeqAccess {
                values: values.into_iter(),
            }),
            other => Err(type_error("array", &other)),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _length: usize, visitor: V) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        length: usize,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_tuple(length, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self {
            Self::Object(values) => visitor.visit_map(ValueMapAccess {
                values: values.into_iter(),
                pending: None,
            }),
            other => Err(type_error("object", &other)),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        match self {
            Self::String(variant) => visitor.visit_enum(variant.into_deserializer()),
            Self::Object(mut values) if values.len() == 1 => {
                let (variant, value) = values.pop_first().expect("length was checked");
                visitor.visit_enum(ValueEnumAccess { variant, value })
            }
            other => Err(type_error("enum string or one-entry object", &other)),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_string(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_unit()
    }
}

fn integer<T: TryFrom<i64>>(value: &Value) -> Result<T> {
    let value = value
        .as_i64()
        .ok_or_else(|| type_error("signed integer", value))?;
    T::try_from(value).map_err(|_| Error::message("integer is out of range"))
}

fn unsigned<T: TryFrom<u64>>(value: &Value) -> Result<T> {
    let value = value
        .as_u64()
        .ok_or_else(|| type_error("unsigned integer", value))?;
    T::try_from(value).map_err(|_| Error::message("integer is out of range"))
}

fn float(value: &Value) -> Result<f64> {
    value.as_f64().ok_or_else(|| type_error("number", value))
}

fn type_error(expected: &str, value: &Value) -> Error {
    let actual = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    Error::message(format!("expected {expected}, found {actual}"))
}

struct ValueSeqAccess {
    values: vec::IntoIter<Value>,
}

impl<'de> SeqAccess<'de> for ValueSeqAccess {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        self.values
            .next()
            .map(|value| seed.deserialize(value))
            .transpose()
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.values.len())
    }
}

struct ValueMapAccess {
    values: btree_map::IntoIter<String, Value>,
    pending: Option<Value>,
}

impl<'de> MapAccess<'de> for ValueMapAccess {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        let Some((key, value)) = self.values.next() else {
            return Ok(None);
        };
        self.pending = Some(value);
        seed.deserialize(key.into_deserializer()).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        seed.deserialize(
            self.pending
                .take()
                .ok_or_else(|| Error::message("map value has no key"))?,
        )
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.values.len())
    }
}

struct ValueEnumAccess {
    variant: String,
    value: Value,
}

impl<'de> EnumAccess<'de> for ValueEnumAccess {
    type Error = Error;
    type Variant = ValueVariantAccess;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant)> {
        let variant = seed.deserialize(serde::de::value::StringDeserializer::<Error>::new(
            self.variant,
        ))?;
        Ok((variant, ValueVariantAccess { value: self.value }))
    }
}

struct ValueVariantAccess {
    value: Value,
}

impl<'de> VariantAccess<'de> for ValueVariantAccess {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        de::Deserialize::deserialize(self.value)
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V: Visitor<'de>>(self, length: usize, visitor: V) -> Result<V::Value> {
        de::Deserializer::deserialize_tuple(self.value, length, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        de::Deserializer::deserialize_struct(self.value, "", fields, visitor)
    }
}
