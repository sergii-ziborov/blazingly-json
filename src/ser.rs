use crate::raw_value::RAW_VALUE_TOKEN;
use crate::{Error, Map, Number, Result, Value};
use serde::ser::{
    self, Impossible, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
};
use serde::Serialize;
use std::fmt;
use std::io::Write;

const ESCAPE_UNICODE: u8 = b'u';

const fn escape_table() -> [u8; 256] {
    let mut table = [0_u8; 256];
    let mut index = 0;
    while index < 0x20 {
        table[index] = ESCAPE_UNICODE;
        index += 1;
    }
    table[8] = b'b';
    table[9] = b't';
    table[10] = b'n';
    table[12] = b'f';
    table[13] = b'r';
    table[34] = b'"';
    table[92] = b'\\';
    table
}

static ESCAPE: [u8; 256] = escape_table();

#[doc(hidden)]
pub trait Formatter {
    fn indent<W: Write>(&mut self, writer: &mut W, depth: usize) -> std::io::Result<()>;
    fn write_colon<W: Write>(&mut self, writer: &mut W) -> std::io::Result<()>;
}

#[doc(hidden)]
#[derive(Default)]
pub struct CompactFormatter;

impl Formatter for CompactFormatter {
    #[inline]
    fn indent<W: Write>(&mut self, _writer: &mut W, _depth: usize) -> std::io::Result<()> {
        Ok(())
    }

    #[inline]
    fn write_colon<W: Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(b":")
    }
}

#[doc(hidden)]
#[derive(Default)]
pub struct PrettyFormatter;

impl Formatter for PrettyFormatter {
    #[inline]
    fn indent<W: Write>(&mut self, writer: &mut W, depth: usize) -> std::io::Result<()> {
        writer.write_all(b"\n")?;
        for _ in 0..depth {
            writer.write_all(b"  ")?;
        }
        Ok(())
    }

    #[inline]
    fn write_colon<W: Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(b": ")
    }
}

struct VecWriter {
    bytes: Vec<u8>,
}

impl VecWriter {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }
}

impl Write for VecWriter {
    #[inline]
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    #[inline]
    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Streaming JSON serializer.
pub struct Serializer<W, F = CompactFormatter> {
    writer: W,
    formatter: F,
    depth: usize,
}

impl<W> Serializer<W, CompactFormatter> {
    #[must_use]
    pub const fn new(writer: W) -> Self {
        Self {
            writer,
            formatter: CompactFormatter,
            depth: 0,
        }
    }
}

impl<W> Serializer<W, PrettyFormatter> {
    #[must_use]
    pub const fn pretty(writer: W) -> Self {
        Self {
            writer,
            formatter: PrettyFormatter,
            depth: 0,
        }
    }
}

impl<W: Write, F: Formatter> Serializer<W, F> {
    pub fn into_inner(self) -> W {
        self.writer
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.write_all(bytes).map_err(Error::from)
    }

    #[inline]
    fn indent(&mut self) -> Result<()> {
        self.formatter
            .indent(&mut self.writer, self.depth)
            .map_err(Error::from)
    }

    #[inline]
    fn write_colon(&mut self) -> Result<()> {
        self.formatter
            .write_colon(&mut self.writer)
            .map_err(Error::from)
    }

    #[inline]
    fn write_string(&mut self, value: &str) -> Result<()> {
        self.write(b"\"")?;
        let bytes = value.as_bytes();
        let mut start = 0;
        for (index, &byte) in bytes.iter().enumerate() {
            let escape = ESCAPE[usize::from(byte)];
            if escape == 0 {
                continue;
            }
            self.write(&bytes[start..index])?;
            if escape == ESCAPE_UNICODE {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                self.write(&[
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    HEX[usize::from(byte >> 4)],
                    HEX[usize::from(byte & 0x0f)],
                ])?;
            } else {
                self.write(&[b'\\', escape])?;
            }
            start = index + 1;
        }
        self.write(&bytes[start..])?;
        self.write(b"\"")
    }

    #[inline]
    fn write_i64(&mut self, value: i64) -> Result<()> {
        self.write(itoa::Buffer::new().format(value).as_bytes())
    }

    #[inline]
    fn write_u64(&mut self, value: u64) -> Result<()> {
        self.write(itoa::Buffer::new().format(value).as_bytes())
    }

    #[inline]
    fn write_f64(&mut self, value: f64) -> Result<()> {
        if value.is_finite() {
            self.write(zmij::Buffer::new().format_finite(value).as_bytes())
        } else {
            self.write(b"null")
        }
    }

    #[inline]
    fn begin(&mut self, byte: u8) -> Result<()> {
        self.write(&[byte])?;
        self.depth += 1;
        Ok(())
    }
}

#[inline]
fn initial_capacity<T: ?Sized>(value: &T, minimum: usize) -> usize {
    if std::mem::size_of::<&T>() > std::mem::size_of::<usize>() {
        let size = std::mem::size_of_val(value).saturating_add(2);
        if size <= 4_096 {
            size.max(minimum)
        } else {
            minimum
        }
    } else {
        minimum
    }
}

/// Serializes a value into a compact byte vector.
///
/// # Errors
///
/// Returns an error when `value` cannot be represented as supported JSON.
#[inline]
pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let mut serializer = Serializer::new(VecWriter::with_capacity(initial_capacity(value, 128)));
    value.serialize(&mut serializer)?;
    Ok(serializer.into_inner().bytes)
}

/// Serializes a value into a compact UTF-8 string.
///
/// # Errors
///
/// Returns an error when `value` cannot be represented as supported JSON.
#[inline]
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    String::from_utf8(to_vec(value)?)
        .map_err(|_| Error::message("serializer emitted invalid UTF-8"))
}

/// Serializes a value into a pretty-printed byte vector.
///
/// # Errors
///
/// Returns an error when `value` cannot be represented as supported JSON.
pub fn to_vec_pretty<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let mut serializer = Serializer::pretty(VecWriter::with_capacity(initial_capacity(value, 256)));
    value.serialize(&mut serializer)?;
    Ok(serializer.into_inner().bytes)
}

/// Serializes a value into a pretty-printed UTF-8 string.
///
/// # Errors
///
/// Returns an error when `value` cannot be represented as supported JSON.
pub fn to_string_pretty<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    String::from_utf8(to_vec_pretty(value)?)
        .map_err(|_| Error::message("serializer emitted invalid UTF-8"))
}

/// Serializes a value into an output writer.
///
/// # Errors
///
/// Returns a serialization error or an error from the output writer.
#[inline]
pub fn to_writer<W: Write, T: Serialize + ?Sized>(writer: W, value: &T) -> Result<()> {
    value.serialize(&mut Serializer::new(writer))
}

impl<'a, W: Write, F: Formatter> ser::Serializer for &'a mut Serializer<W, F> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Compound<'a, W, F>;
    type SerializeTuple = Compound<'a, W, F>;
    type SerializeTupleStruct = Compound<'a, W, F>;
    type SerializeTupleVariant = Compound<'a, W, F>;
    type SerializeMap = Compound<'a, W, F>;
    type SerializeStruct = Compound<'a, W, F>;
    type SerializeStructVariant = Compound<'a, W, F>;

    #[inline]
    fn serialize_bool(self, value: bool) -> Result<()> {
        self.write(if value { b"true" } else { b"false" })
    }

    #[inline]
    fn serialize_i8(self, value: i8) -> Result<()> {
        self.write_i64(i64::from(value))
    }

    #[inline]
    fn serialize_i16(self, value: i16) -> Result<()> {
        self.write_i64(i64::from(value))
    }

    #[inline]
    fn serialize_i32(self, value: i32) -> Result<()> {
        self.write_i64(i64::from(value))
    }

    #[inline]
    fn serialize_i64(self, value: i64) -> Result<()> {
        self.write_i64(value)
    }

    fn serialize_i128(self, value: i128) -> Result<()> {
        self.write(itoa::Buffer::new().format(value).as_bytes())
    }

    #[inline]
    fn serialize_u8(self, value: u8) -> Result<()> {
        self.write_u64(u64::from(value))
    }

    #[inline]
    fn serialize_u16(self, value: u16) -> Result<()> {
        self.write_u64(u64::from(value))
    }

    #[inline]
    fn serialize_u32(self, value: u32) -> Result<()> {
        self.write_u64(u64::from(value))
    }

    #[inline]
    fn serialize_u64(self, value: u64) -> Result<()> {
        self.write_u64(value)
    }

    fn serialize_u128(self, value: u128) -> Result<()> {
        self.write(itoa::Buffer::new().format(value).as_bytes())
    }

    #[inline]
    fn serialize_f32(self, value: f32) -> Result<()> {
        self.write_f64(f64::from(value))
    }

    #[inline]
    fn serialize_f64(self, value: f64) -> Result<()> {
        self.write_f64(value)
    }

    fn serialize_char(self, value: char) -> Result<()> {
        self.write_string(value.encode_utf8(&mut [0; 4]))
    }

    #[inline]
    fn serialize_str(self, value: &str) -> Result<()> {
        self.write_string(value)
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<()> {
        let mut sequence = self.serialize_seq(Some(value.len()))?;
        for byte in value {
            SerializeSeq::serialize_element(&mut sequence, byte)?;
        }
        SerializeSeq::end(sequence)
    }

    #[inline]
    fn serialize_none(self) -> Result<()> {
        self.serialize_unit()
    }

    #[inline]
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }

    #[inline]
    fn serialize_unit(self) -> Result<()> {
        self.write(b"null")
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.write_string(variant)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()> {
        self.begin(b'{')?;
        self.indent()?;
        self.write_string(variant)?;
        self.write_colon()?;
        value.serialize(&mut *self)?;
        self.depth -= 1;
        self.indent()?;
        self.write(b"}")
    }

    #[inline]
    fn serialize_seq(self, length: Option<usize>) -> Result<Self::SerializeSeq> {
        self.begin(b'[')?;
        Ok(Compound::new(self, Kind::Array, length, false))
    }

    fn serialize_tuple(self, length: usize) -> Result<Self::SerializeTuple> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        length: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        length: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.begin(b'{')?;
        self.indent()?;
        self.write_string(variant)?;
        self.write_colon()?;
        self.begin(b'[')?;
        Ok(Compound::new(self, Kind::Array, Some(length), true))
    }

    #[inline]
    fn serialize_map(self, length: Option<usize>) -> Result<Self::SerializeMap> {
        self.begin(b'{')?;
        Ok(Compound::new(self, Kind::Object, length, false))
    }

    #[inline]
    fn serialize_struct(self, name: &'static str, length: usize) -> Result<Self::SerializeStruct> {
        if name == RAW_VALUE_TOKEN {
            Ok(Compound::raw(self))
        } else {
            self.serialize_map(Some(length))
        }
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        length: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.begin(b'{')?;
        self.indent()?;
        self.write_string(variant)?;
        self.write_colon()?;
        self.begin(b'{')?;
        Ok(Compound::new(self, Kind::Object, Some(length), true))
    }

    fn collect_str<T: fmt::Display + ?Sized>(self, value: &T) -> Result<()> {
        self.write_string(&value.to_string())
    }

    fn is_human_readable(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Array,
    Object,
    RawValue,
}

#[derive(Clone, Copy, PartialEq)]
enum CompoundState {
    Empty,
    Complete,
    KeyPending,
}

#[derive(Clone, Copy)]
enum Wrapper {
    None,
    Object,
}

pub struct Compound<'a, W, F> {
    serializer: &'a mut Serializer<W, F>,
    kind: Kind,
    state: CompoundState,
    wrapper: Wrapper,
    _length: Option<usize>,
}

impl<'a, W: Write, F: Formatter> Compound<'a, W, F> {
    fn new(
        serializer: &'a mut Serializer<W, F>,
        kind: Kind,
        length: Option<usize>,
        outer_object: bool,
    ) -> Self {
        Self {
            serializer,
            kind,
            state: CompoundState::Empty,
            wrapper: if outer_object {
                Wrapper::Object
            } else {
                Wrapper::None
            },
            _length: length,
        }
    }

    #[inline]
    fn raw(serializer: &'a mut Serializer<W, F>) -> Self {
        Self {
            serializer,
            kind: Kind::RawValue,
            state: CompoundState::Empty,
            wrapper: Wrapper::None,
            _length: Some(1),
        }
    }

    #[inline]
    fn separator(&mut self) -> Result<()> {
        if self.kind == Kind::RawValue {
            return Err(invalid_raw_value());
        }
        if self.state != CompoundState::Empty {
            self.serializer.write(b",")?;
        }
        self.serializer.indent()?;
        self.state = CompoundState::Complete;
        Ok(())
    }

    #[inline]
    fn key(&mut self, key: &str) -> Result<()> {
        self.separator()?;
        self.serializer.write_string(key)?;
        self.serializer.write_colon()?;
        self.state = CompoundState::KeyPending;
        Ok(())
    }

    #[inline]
    fn finish(self) -> Result<()> {
        if self.kind == Kind::RawValue {
            return if self.state == CompoundState::Complete {
                Ok(())
            } else {
                Err(invalid_raw_value())
            };
        }
        if self.state == CompoundState::KeyPending {
            return Err(Error::message("map key has no value"));
        }
        self.serializer.depth -= 1;
        if self.state != CompoundState::Empty {
            self.serializer.indent()?;
        }
        self.serializer.write(match self.kind {
            Kind::Array => b"]",
            Kind::Object => b"}",
            Kind::RawValue => unreachable!("raw value handled before delimiter"),
        })?;
        if matches!(self.wrapper, Wrapper::Object) {
            self.serializer.depth -= 1;
            self.serializer.indent()?;
            self.serializer.write(b"}")?;
        }
        Ok(())
    }
}

impl<W: Write, F: Formatter> SerializeSeq for Compound<'_, W, F> {
    type Ok = ();
    type Error = Error;

    #[inline]
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.separator()?;
        value.serialize(&mut *self.serializer)
    }

    fn end(self) -> Result<()> {
        self.finish()
    }
}

impl<W: Write, F: Formatter> SerializeTuple for Compound<'_, W, F> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<()> {
        self.finish()
    }
}

impl<W: Write, F: Formatter> SerializeTupleStruct for Compound<'_, W, F> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<()> {
        self.finish()
    }
}

impl<W: Write, F: Formatter> SerializeTupleVariant for Compound<'_, W, F> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<()> {
        self.finish()
    }
}

impl<W: Write, F: Formatter> SerializeMap for Compound<'_, W, F> {
    type Ok = ();
    type Error = Error;

    #[inline]
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<()> {
        self.separator()?;
        key.serialize(WriteKeySerializer {
            serializer: &mut *self.serializer,
        })?;
        self.serializer.write_colon()?;
        self.state = CompoundState::KeyPending;
        Ok(())
    }

    #[inline]
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        if self.state != CompoundState::KeyPending {
            return Err(Error::message("map value has no key"));
        }
        self.state = CompoundState::Complete;
        value.serialize(&mut *self.serializer)
    }

    fn end(self) -> Result<()> {
        self.finish()
    }
}

impl<W: Write, F: Formatter> SerializeStruct for Compound<'_, W, F> {
    type Ok = ();
    type Error = Error;

    #[inline]
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        if self.kind == Kind::RawValue {
            if key != RAW_VALUE_TOKEN || self.state != CompoundState::Empty {
                return Err(invalid_raw_value());
            }
            value.serialize(RawValueStrEmitter {
                serializer: &mut *self.serializer,
            })?;
            self.state = CompoundState::Complete;
            return Ok(());
        }
        self.key(key)?;
        self.state = CompoundState::Complete;
        value.serialize(&mut *self.serializer)
    }

    fn end(self) -> Result<()> {
        self.finish()
    }
}

impl<W: Write, F: Formatter> SerializeStructVariant for Compound<'_, W, F> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        SerializeStruct::serialize_field(self, key, value)
    }

    fn end(self) -> Result<()> {
        self.finish()
    }
}

#[inline]
fn invalid_raw_value() -> Error {
    Error::message("invalid RawValue serialization")
}

struct RawValueStrEmitter<'a, W, F> {
    serializer: &'a mut Serializer<W, F>,
}

macro_rules! reject_raw_value_scalars {
    ($($method:ident($type:ty)),+ $(,)?) => {
        $(
            fn $method(self, _value: $type) -> Result<()> {
                Err(invalid_raw_value())
            }
        )+
    };
}

impl<W: Write, F: Formatter> ser::Serializer for RawValueStrEmitter<'_, W, F> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Impossible<(), Error>;
    type SerializeTuple = Impossible<(), Error>;
    type SerializeTupleStruct = Impossible<(), Error>;
    type SerializeTupleVariant = Impossible<(), Error>;
    type SerializeMap = Impossible<(), Error>;
    type SerializeStruct = Impossible<(), Error>;
    type SerializeStructVariant = Impossible<(), Error>;

    reject_raw_value_scalars!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_i128(i128),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_u128(u128),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char)
    );

    #[inline]
    fn serialize_str(self, value: &str) -> Result<()> {
        self.serializer.write(value.as_bytes())
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<()> {
        Err(invalid_raw_value())
    }

    fn serialize_none(self) -> Result<()> {
        Err(invalid_raw_value())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, _value: &T) -> Result<()> {
        Err(invalid_raw_value())
    }

    fn serialize_unit(self) -> Result<()> {
        Err(invalid_raw_value())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Err(invalid_raw_value())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        Err(invalid_raw_value())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _value: &T,
    ) -> Result<()> {
        Err(invalid_raw_value())
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()> {
        Err(invalid_raw_value())
    }

    fn serialize_seq(self, _length: Option<usize>) -> Result<Self::SerializeSeq> {
        Err(invalid_raw_value())
    }

    fn serialize_tuple(self, _length: usize) -> Result<Self::SerializeTuple> {
        Err(invalid_raw_value())
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Err(invalid_raw_value())
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(invalid_raw_value())
    }

    fn serialize_map(self, _length: Option<usize>) -> Result<Self::SerializeMap> {
        Err(invalid_raw_value())
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStruct> {
        Err(invalid_raw_value())
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(invalid_raw_value())
    }

    fn collect_str<T: fmt::Display + ?Sized>(self, _value: &T) -> Result<()> {
        Err(invalid_raw_value())
    }
}

struct WriteKeySerializer<'a, W, F> {
    serializer: &'a mut Serializer<W, F>,
}

macro_rules! write_key_integer {
    ($($method:ident($type:ty)),+ $(,)?) => {
        $(
            #[inline]
            fn $method(self, value: $type) -> Result<()> {
                let mut buffer = itoa::Buffer::new();
                self.serializer.write_string(buffer.format(value))
            }
        )+
    };
}

impl<W: Write, F: Formatter> ser::Serializer for WriteKeySerializer<'_, W, F> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Impossible<(), Error>;
    type SerializeTuple = Impossible<(), Error>;
    type SerializeTupleStruct = Impossible<(), Error>;
    type SerializeTupleVariant = Impossible<(), Error>;
    type SerializeMap = Impossible<(), Error>;
    type SerializeStruct = Impossible<(), Error>;
    type SerializeStructVariant = Impossible<(), Error>;

    #[inline]
    fn serialize_bool(self, value: bool) -> Result<()> {
        self.serializer
            .write_string(if value { "true" } else { "false" })
    }

    write_key_integer!(
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_i128(i128),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_u128(u128)
    );

    fn serialize_f32(self, _value: f32) -> Result<()> {
        Err(Error::message("floating-point map keys are not supported"))
    }

    fn serialize_f64(self, _value: f64) -> Result<()> {
        Err(Error::message("floating-point map keys are not supported"))
    }

    #[inline]
    fn serialize_char(self, value: char) -> Result<()> {
        self.serializer.write_string(value.encode_utf8(&mut [0; 4]))
    }

    #[inline]
    fn serialize_str(self, value: &str) -> Result<()> {
        self.serializer.write_string(value)
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<()> {
        Err(Error::message("byte array map keys are not supported"))
    }

    fn serialize_none(self) -> Result<()> {
        Err(Error::message("null map keys are not supported"))
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        Err(Error::message("null map keys are not supported"))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.serializer.write_string(variant)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()> {
        Err(Error::message("complex map keys are not supported"))
    }

    fn serialize_seq(self, _length: Option<usize>) -> Result<Self::SerializeSeq> {
        Err(Error::message("sequence map keys are not supported"))
    }

    fn serialize_tuple(self, _length: usize) -> Result<Self::SerializeTuple> {
        Err(Error::message("tuple map keys are not supported"))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Err(Error::message("tuple map keys are not supported"))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(Error::message("tuple map keys are not supported"))
    }

    fn serialize_map(self, _length: Option<usize>) -> Result<Self::SerializeMap> {
        Err(Error::message("map keys cannot be maps"))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStruct> {
        Err(Error::message("struct map keys are not supported"))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(Error::message("struct map keys are not supported"))
    }
}

struct KeySerializer;

macro_rules! key_integer {
    ($($method:ident($type:ty)),+ $(,)?) => {
        $(
            #[inline]
            fn $method(self, value: $type) -> Result<Self::Ok> {
                Ok(value.to_string())
            }
        )+
    };
}

impl ser::Serializer for KeySerializer {
    type Ok = String;
    type Error = Error;
    type SerializeSeq = Impossible<String, Error>;
    type SerializeTuple = Impossible<String, Error>;
    type SerializeTupleStruct = Impossible<String, Error>;
    type SerializeTupleVariant = Impossible<String, Error>;
    type SerializeMap = Impossible<String, Error>;
    type SerializeStruct = Impossible<String, Error>;
    type SerializeStructVariant = Impossible<String, Error>;

    #[inline]
    fn serialize_bool(self, value: bool) -> Result<String> {
        Ok(value.to_string())
    }

    key_integer!(
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_i128(i128),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_u128(u128)
    );

    fn serialize_f32(self, _value: f32) -> Result<String> {
        Err(Error::message("floating-point map keys are not supported"))
    }

    fn serialize_f64(self, _value: f64) -> Result<String> {
        Err(Error::message("floating-point map keys are not supported"))
    }

    #[inline]
    fn serialize_char(self, value: char) -> Result<String> {
        Ok(value.to_string())
    }

    #[inline]
    fn serialize_str(self, value: &str) -> Result<String> {
        Ok(value.to_owned())
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<String> {
        Err(Error::message("byte array map keys are not supported"))
    }

    fn serialize_none(self) -> Result<String> {
        Err(Error::message("null map keys are not supported"))
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<String> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<String> {
        Err(Error::message("null map keys are not supported"))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<String> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<String> {
        Ok(variant.to_owned())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<String> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<String> {
        Err(Error::message("complex map keys are not supported"))
    }

    fn serialize_seq(self, _length: Option<usize>) -> Result<Self::SerializeSeq> {
        Err(Error::message("sequence map keys are not supported"))
    }

    fn serialize_tuple(self, _length: usize) -> Result<Self::SerializeTuple> {
        Err(Error::message("tuple map keys are not supported"))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Err(Error::message("tuple map keys are not supported"))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(Error::message("tuple map keys are not supported"))
    }

    fn serialize_map(self, _length: Option<usize>) -> Result<Self::SerializeMap> {
        Err(Error::message("map keys cannot be maps"))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStruct> {
        Err(Error::message("struct map keys are not supported"))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(Error::message("struct map keys are not supported"))
    }
}

/// Serializes directly into an owned [`Value`] without an intermediate string.
///
/// # Errors
///
/// Returns an error when `value` cannot be represented by the supported DOM.
pub fn to_value<T: Serialize>(value: T) -> Result<Value> {
    value.serialize(ValueSerializer)
}

struct ValueSerializer;

impl ser::Serializer for ValueSerializer {
    type Ok = Value;
    type Error = Error;
    type SerializeSeq = ValueCompound;
    type SerializeTuple = ValueCompound;
    type SerializeTupleStruct = ValueCompound;
    type SerializeTupleVariant = ValueCompound;
    type SerializeMap = ValueCompound;
    type SerializeStruct = ValueCompound;
    type SerializeStructVariant = ValueCompound;

    fn serialize_bool(self, value: bool) -> Result<Value> {
        Ok(Value::Bool(value))
    }

    fn serialize_i8(self, value: i8) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_i128(self, value: i128) -> Result<Value> {
        let value =
            i64::try_from(value).map_err(|_| Error::message("i128 is outside JSON range"))?;
        Ok(Value::from(value))
    }

    fn serialize_u8(self, value: u8) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<Value> {
        Ok(Value::from(value))
    }

    fn serialize_u128(self, value: u128) -> Result<Value> {
        let value =
            u64::try_from(value).map_err(|_| Error::message("u128 is outside JSON range"))?;
        Ok(Value::from(value))
    }

    fn serialize_f32(self, value: f32) -> Result<Value> {
        self.serialize_f64(f64::from(value))
    }

    fn serialize_f64(self, value: f64) -> Result<Value> {
        Ok(Number::from_f64(value).map_or(Value::Null, Value::Number))
    }

    fn serialize_char(self, value: char) -> Result<Value> {
        Ok(Value::String(value.to_string()))
    }

    fn serialize_str(self, value: &str) -> Result<Value> {
        Ok(Value::String(value.to_owned()))
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Value> {
        Ok(Value::Array(
            value.iter().copied().map(Value::from).collect(),
        ))
    }

    fn serialize_none(self) -> Result<Value> {
        Ok(Value::Null)
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Value> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Value> {
        Ok(Value::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Value> {
        Ok(Value::Null)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Value> {
        Ok(Value::String(variant.to_owned()))
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Value> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value> {
        let mut map = Map::new();
        map.insert(variant.to_owned(), value.serialize(Self)?);
        Ok(Value::Object(map))
    }

    fn serialize_seq(self, length: Option<usize>) -> Result<ValueCompound> {
        Ok(ValueCompound::array(length, None))
    }

    fn serialize_tuple(self, length: usize) -> Result<ValueCompound> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_struct(self, _name: &'static str, length: usize) -> Result<ValueCompound> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        length: usize,
    ) -> Result<ValueCompound> {
        Ok(ValueCompound::array(Some(length), Some(variant.to_owned())))
    }

    fn serialize_map(self, _length: Option<usize>) -> Result<ValueCompound> {
        Ok(ValueCompound::object(None))
    }

    fn serialize_struct(self, name: &'static str, _length: usize) -> Result<ValueCompound> {
        if name == RAW_VALUE_TOKEN {
            Ok(ValueCompound::raw())
        } else {
            Ok(ValueCompound::object(None))
        }
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _length: usize,
    ) -> Result<ValueCompound> {
        Ok(ValueCompound::object(Some(variant.to_owned())))
    }

    fn collect_str<T: fmt::Display + ?Sized>(self, value: &T) -> Result<Value> {
        Ok(Value::String(value.to_string()))
    }
}

enum ValueCompoundKind {
    Array(Vec<Value>),
    Object(Map<String, Value>),
    RawValue(Option<Value>),
}

struct ValueCompound {
    kind: ValueCompoundKind,
    pending_key: Option<String>,
    variant: Option<String>,
}

impl ValueCompound {
    fn array(length: Option<usize>, variant: Option<String>) -> Self {
        Self {
            kind: ValueCompoundKind::Array(Vec::with_capacity(length.unwrap_or(0))),
            pending_key: None,
            variant,
        }
    }

    fn object(variant: Option<String>) -> Self {
        Self {
            kind: ValueCompoundKind::Object(Map::new()),
            pending_key: None,
            variant,
        }
    }

    fn raw() -> Self {
        Self {
            kind: ValueCompoundKind::RawValue(None),
            pending_key: None,
            variant: None,
        }
    }

    fn push<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        match &mut self.kind {
            ValueCompoundKind::Array(values) => {
                values.push(value.serialize(ValueSerializer)?);
                Ok(())
            }
            ValueCompoundKind::Object(_) => Err(Error::message("expected an array")),
            ValueCompoundKind::RawValue(_) => Err(invalid_raw_value()),
        }
    }

    fn insert<T: Serialize + ?Sized>(&mut self, key: String, value: &T) -> Result<()> {
        match &mut self.kind {
            ValueCompoundKind::Object(values) => {
                values.insert(key, value.serialize(ValueSerializer)?);
                Ok(())
            }
            ValueCompoundKind::Array(_) => Err(Error::message("expected an object")),
            ValueCompoundKind::RawValue(_) => Err(invalid_raw_value()),
        }
    }

    fn finish(self) -> Result<Value> {
        if self.pending_key.is_some() {
            return Err(Error::message("map key has no value"));
        }
        let value = match self.kind {
            ValueCompoundKind::Array(values) => Value::Array(values),
            ValueCompoundKind::Object(values) => Value::Object(values),
            ValueCompoundKind::RawValue(value) => {
                return value.ok_or_else(invalid_raw_value);
            }
        };
        if let Some(variant) = self.variant {
            Ok(Value::Object(Map::from([(variant, value)])))
        } else {
            Ok(value)
        }
    }
}

impl SerializeSeq for ValueCompound {
    type Ok = Value;
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }

    fn end(self) -> Result<Value> {
        self.finish()
    }
}

impl SerializeTuple for ValueCompound {
    type Ok = Value;
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }

    fn end(self) -> Result<Value> {
        self.finish()
    }
}

impl SerializeTupleStruct for ValueCompound {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }

    fn end(self) -> Result<Value> {
        self.finish()
    }
}

impl SerializeTupleVariant for ValueCompound {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }

    fn end(self) -> Result<Value> {
        self.finish()
    }
}

impl SerializeMap for ValueCompound {
    type Ok = Value;
    type Error = Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<()> {
        if self.pending_key.is_some() {
            return Err(Error::message("map key has no value"));
        }
        self.pending_key = Some(key.serialize(KeySerializer)?);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        let key = self
            .pending_key
            .take()
            .ok_or_else(|| Error::message("map value has no key"))?;
        self.insert(key, value)
    }

    fn end(self) -> Result<Value> {
        self.finish()
    }
}

impl SerializeStruct for ValueCompound {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        match &mut self.kind {
            ValueCompoundKind::RawValue(output) => {
                if key != RAW_VALUE_TOKEN || output.is_some() {
                    return Err(invalid_raw_value());
                }
                let serialized = value.serialize(ValueSerializer)?;
                let Value::String(raw) = serialized else {
                    return Err(invalid_raw_value());
                };
                *output = Some(crate::from_str(&raw)?);
                Ok(())
            }
            _ => self.insert(key.to_owned(), value),
        }
    }

    fn end(self) -> Result<Value> {
        self.finish()
    }
}

impl SerializeStructVariant for ValueCompound {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.insert(key.to_owned(), value)
    }

    fn end(self) -> Result<Value> {
        self.finish()
    }
}
