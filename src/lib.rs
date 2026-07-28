//! Focused JSON parsing and encoding for Blazingly protocol workloads.

mod de;
mod error;
mod number;
mod ser;
mod value;
mod value_de;

pub use de::{Deserializer, from_slice, from_str};
pub use error::{Error, Result};
pub use number::Number;
pub use ser::{Serializer, to_string, to_string_pretty, to_value, to_vec, to_writer};
pub use value::{Value, ValueIndex, from_value};

/// Object map used by [`Value`].
pub type Map<K, V> = std::collections::BTreeMap<K, V>;

#[doc(hidden)]
#[macro_export]
macro_rules! __json_array {
    ($values:ident;) => {};
    ($values:ident; null $(, $($rest:tt)*)?) => {
        $values.push($crate::Value::Null);
        $crate::__json_array!($values; $($($rest)*)?);
    };
    ($values:ident; [$($array:tt)*] $(, $($rest:tt)*)?) => {
        $values.push($crate::json!([$($array)*]));
        $crate::__json_array!($values; $($($rest)*)?);
    };
    ($values:ident; {$($object:tt)*} $(, $($rest:tt)*)?) => {
        $values.push($crate::json!({$($object)*}));
        $crate::__json_array!($values; $($($rest)*)?);
    };
    ($values:ident; $value:expr $(, $($rest:tt)*)?) => {
        $values.push($crate::to_value(&$value).expect("JSON value serialization failed"));
        $crate::__json_array!($values; $($($rest)*)?);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __json_object {
    ($object:ident;) => {};
    ($object:ident; $key:literal : null $(, $($rest:tt)*)?) => {
        $object.insert(($key).into(), $crate::Value::Null);
        $crate::__json_object!($object; $($($rest)*)?);
    };
    ($object:ident; $key:literal : [$($array:tt)*] $(, $($rest:tt)*)?) => {
        $object.insert(($key).into(), $crate::json!([$($array)*]));
        $crate::__json_object!($object; $($($rest)*)?);
    };
    ($object:ident; $key:literal : {$($nested:tt)*} $(, $($rest:tt)*)?) => {
        $object.insert(($key).into(), $crate::json!({$($nested)*}));
        $crate::__json_object!($object; $($($rest)*)?);
    };
    ($object:ident; $key:literal : $value:expr $(, $($rest:tt)*)?) => {
        $object.insert(
            ($key).into(),
            $crate::to_value(&$value).expect("JSON value serialization failed"),
        );
        $crate::__json_object!($object; $($($rest)*)?);
    };
}

/// Constructs a [`Value`] with JSON-like syntax.
#[macro_export]
macro_rules! json {
    (null) => {
        $crate::Value::Null
    };
    ([$($tokens:tt)*]) => {{
        let mut values = ::std::vec::Vec::new();
        $crate::__json_array!(values; $($tokens)*);
        $crate::Value::Array(values)
    }};
    ({$($tokens:tt)*}) => {{
        let mut object = $crate::Map::new();
        $crate::__json_object!(object; $($tokens)*);
        $crate::Value::Object(object)
    }};
    ($value:expr) => {
        $crate::to_value(&$value).expect("JSON value serialization failed")
    };
}
