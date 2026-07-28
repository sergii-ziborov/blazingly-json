use serde::{Deserialize, Serialize};
use std::fmt;

/// A finite JSON number represented without losing signed or unsigned integers.
#[derive(Clone, Copy, Debug)]
pub struct Number(Repr);

#[derive(Clone, Copy, Debug)]
enum Repr {
    I64(i64),
    U64(u64),
    F64(f64),
}

impl Number {
    /// Creates a JSON number from a finite float.
    #[must_use]
    pub fn from_f64(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(Repr::F64(value)))
    }

    #[must_use]
    pub fn is_i64(&self) -> bool {
        self.as_i64().is_some()
    }

    #[must_use]
    pub const fn is_u64(&self) -> bool {
        matches!(self.0, Repr::U64(_))
    }

    #[must_use]
    pub const fn is_f64(&self) -> bool {
        matches!(self.0, Repr::F64(_))
    }

    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self.0 {
            Repr::I64(value) => Some(value),
            Repr::U64(value) => i64::try_from(value).ok(),
            Repr::F64(_) => None,
        }
    }

    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self.0 {
            Repr::I64(value) => u64::try_from(value).ok(),
            Repr::U64(value) => Some(value),
            Repr::F64(_) => None,
        }
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_f64(&self) -> Option<f64> {
        match self.0 {
            Repr::I64(value) => Some(value as f64),
            Repr::U64(value) => Some(value as f64),
            Repr::F64(value) => Some(value),
        }
    }
}

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        match (self.0, other.0) {
            (Repr::I64(left), Repr::I64(right)) => left == right,
            (Repr::U64(left), Repr::U64(right)) => left == right,
            (Repr::F64(left), Repr::F64(right)) => left == right,
            _ => false,
        }
    }
}

impl Eq for Number {}

impl fmt::Display for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Repr::I64(value) => value.fmt(formatter),
            Repr::U64(value) => value.fmt(formatter),
            Repr::F64(value) => formatter.write_str(zmij::Buffer::new().format_finite(value)),
        }
    }
}

impl Serialize for Number {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Repr::I64(value) => serializer.serialize_i64(value),
            Repr::U64(value) => serializer.serialize_u64(value),
            Repr::F64(value) => serializer.serialize_f64(value),
        }
    }
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NumberVisitor;

        impl serde::de::Visitor<'_> for NumberVisitor {
            type Value = Number;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON number")
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(Number::from(value))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Number::from(value))
            }

            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Number::from_f64(value).ok_or_else(|| E::custom("non-finite JSON number"))
            }
        }

        deserializer.deserialize_any(NumberVisitor)
    }
}

macro_rules! from_signed {
    ($($type:ty),+ $(,)?) => {
        $(
            impl From<$type> for Number {
                fn from(value: $type) -> Self {
                    let value = i64::try_from(value).expect("signed primitive fits in i64");
                    if value < 0 {
                        Self(Repr::I64(value))
                    } else {
                        Self(Repr::U64(
                            u64::try_from(value).expect("nonnegative i64 fits in u64"),
                        ))
                    }
                }
            }
        )+
    };
}

macro_rules! from_unsigned {
    ($($type:ty),+ $(,)?) => {
        $(
            impl From<$type> for Number {
                fn from(value: $type) -> Self {
                    Self(Repr::U64(u64::try_from(value).expect("unsigned primitive fits in u64")))
                }
            }
        )+
    };
}

from_signed!(i8, i16, i32, i64, isize);
from_unsigned!(u8, u16, u32, u64, usize);
