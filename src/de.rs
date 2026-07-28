use crate::raw::RAW_JSON_TOKEN;
use crate::{Error, Result};
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use serde::Deserialize;
use std::borrow::Cow;
use std::fmt;
use std::str;

const MAX_DEPTH: usize = 128;
const STRING_WORD_BYTES: usize = std::mem::size_of::<u64>();
const ONE_BYTES: u64 = u64::MAX / 255;
const HIGH_BYTES: u64 = ONE_BYTES << 7;

#[inline]
fn find_string_special(bytes: &[u8]) -> Option<usize> {
    let mut offset = 0;
    let mut chunks = bytes.chunks_exact(STRING_WORD_BYTES);

    for chunk in &mut chunks {
        let word = u64::from_le_bytes(chunk.try_into().expect("u64-sized chunk"));
        let contains_control = word.wrapping_sub(ONE_BYTES * 0x20) & !word;
        let quote = word ^ (ONE_BYTES * u64::from(b'"'));
        let contains_quote = quote.wrapping_sub(ONE_BYTES) & !quote;
        let backslash = word ^ (ONE_BYTES * u64::from(b'\\'));
        let contains_backslash = backslash.wrapping_sub(ONE_BYTES) & !backslash;
        let special = (contains_control | contains_quote | contains_backslash) & HIGH_BYTES;

        if special != 0 {
            return Some(offset + special.trailing_zeros() as usize / 8);
        }
        offset += STRING_WORD_BYTES;
    }

    chunks
        .remainder()
        .iter()
        .position(|&byte| byte < 0x20 || matches!(byte, b'"' | b'\\'))
        .map(|relative| offset + relative)
}

/// A zero-copy Serde deserializer over a UTF-8 JSON slice.
pub struct Deserializer<'de> {
    input: &'de [u8],
    source: Option<&'de str>,
    index: usize,
    depth: usize,
}

/// A low-level, allocation-free cursor for routing selected object fields.
///
/// Prefer normal Serde deserialization for application models. `Cursor` is
/// intended for protocol envelopes whose hot path must inspect a few fields
/// and defer large nested values as [`crate::RawJson`].
pub struct Cursor<'de> {
    deserializer: Deserializer<'de>,
}

/// Streaming view over one JSON object visited by [`Cursor::object`].
pub struct Object<'cursor, 'de> {
    deserializer: &'cursor mut Deserializer<'de>,
    first: bool,
    finished: bool,
}

/// One object member. The value must be consumed with one of this type's
/// methods before requesting the next field.
pub struct Field<'cursor, 'de> {
    name: Cow<'de, str>,
    deserializer: &'cursor mut Deserializer<'de>,
}

impl<'de> Cursor<'de> {
    /// Creates a cursor over a byte slice.
    #[must_use]
    pub const fn from_slice(input: &'de [u8]) -> Self {
        Self {
            deserializer: Deserializer::from_slice(input),
        }
    }

    /// Creates a cursor over a UTF-8 string.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(input: &'de str) -> Self {
        Self {
            deserializer: Deserializer::from_str(input),
        }
    }

    /// Visits the next value as an object.
    ///
    /// Unvisited fields are validated and skipped before this method returns.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON or when the next value is not an
    /// object.
    pub fn object<T>(
        &mut self,
        visitor: impl FnOnce(&mut Object<'_, 'de>) -> Result<T>,
    ) -> Result<T> {
        visit_object(&mut self.deserializer, visitor)
    }

    /// Verifies that no trailing value or garbage remains.
    ///
    /// # Errors
    ///
    /// Returns an error when non-whitespace input remains.
    pub fn end(&mut self) -> Result<()> {
        self.deserializer.end()
    }
}

impl<'de> Object<'_, 'de> {
    /// Returns the next field, or `None` after the closing brace.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed object syntax.
    pub fn next_field(&mut self) -> Result<Option<Field<'_, 'de>>> {
        if self.finished {
            return Ok(None);
        }
        self.deserializer.skip_whitespace();
        if self.deserializer.input.get(self.deserializer.index) == Some(&b'}') {
            self.deserializer.index += 1;
            self.deserializer.leave();
            self.finished = true;
            return Ok(None);
        }
        if self.first {
            self.first = false;
        } else {
            self.deserializer.expect_byte(b',')?;
            if self.deserializer.peek() == Some(b'}') {
                return Err(self.deserializer.error("trailing comma in object"));
            }
        }
        let name = self.deserializer.parse_string()?;
        self.deserializer.expect_byte(b':')?;
        Ok(Some(Field {
            name,
            deserializer: self.deserializer,
        }))
    }

    fn finish(&mut self) -> Result<()> {
        while let Some(field) = self.next_field()? {
            field.skip()?;
        }
        Ok(())
    }
}

impl Drop for Object<'_, '_> {
    fn drop(&mut self) {
        if !self.finished {
            self.deserializer.leave();
        }
    }
}

impl<'de> Field<'_, 'de> {
    /// Returns this member's decoded field name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Skips and validates this member's value without constructing it.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON.
    pub fn skip(self) -> Result<()> {
        self.deserializer.skip_value()
    }

    /// Captures this member's value without allocating.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON.
    pub fn raw(self) -> Result<crate::RawJson<'de>> {
        self.deserializer.capture_raw().map(crate::RawJson::new)
    }

    /// Decodes this member as a string, borrowing when it has no escapes.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is not a valid JSON string.
    pub fn string(self) -> Result<Cow<'de, str>> {
        self.deserializer.parse_string()
    }

    /// Deserializes this member into a Serde type.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON or a type mismatch.
    pub fn deserialize<T: Deserialize<'de>>(self) -> Result<T> {
        T::deserialize(&mut *self.deserializer)
    }

    /// Visits this member as a nested object.
    ///
    /// Unvisited nested fields are validated and skipped.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON or when the value is not an object.
    pub fn object<T>(self, visitor: impl FnOnce(&mut Object<'_, 'de>) -> Result<T>) -> Result<T> {
        visit_object(self.deserializer, visitor)
    }
}

fn visit_object<'de, T>(
    deserializer: &mut Deserializer<'de>,
    visitor: impl FnOnce(&mut Object<'_, 'de>) -> Result<T>,
) -> Result<T> {
    deserializer.expect_byte(b'{')?;
    deserializer.enter()?;
    let mut object = Object {
        deserializer,
        first: true,
        finished: false,
    };
    let value = visitor(&mut object)?;
    object.finish()?;
    Ok(value)
}

impl<'de> Deserializer<'de> {
    #[must_use]
    pub const fn from_slice(input: &'de [u8]) -> Self {
        Self {
            input,
            source: None,
            index: 0,
            depth: 0,
        }
    }

    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(input: &'de str) -> Self {
        Self {
            input: input.as_bytes(),
            source: Some(input),
            index: 0,
            depth: 0,
        }
    }

    /// Verifies that the input contains no second value or trailing garbage.
    ///
    /// # Errors
    ///
    /// Returns an error when non-whitespace input remains.
    pub fn end(&mut self) -> Result<()> {
        self.skip_whitespace();
        if self.index == self.input.len() {
            Ok(())
        } else {
            Err(self.error("trailing characters"))
        }
    }

    fn error(&self, message: impl Into<String>) -> Error {
        Error::syntax(message, self.input, self.index)
    }

    fn utf8_source(&mut self) -> Result<&'de str> {
        if let Some(source) = self.source {
            return Ok(source);
        }
        let source =
            str::from_utf8(self.input).map_err(|_| self.error("input is not valid UTF-8"))?;
        self.source = Some(source);
        Ok(source)
    }

    #[inline]
    fn peek(&mut self) -> Option<u8> {
        self.skip_whitespace();
        self.input.get(self.index).copied()
    }

    #[inline]
    fn skip_whitespace(&mut self) {
        while matches!(
            self.input.get(self.index),
            Some(b' ' | b'\n' | b'\r' | b'\t')
        ) {
            self.index += 1;
        }
    }

    #[inline]
    fn expect_byte(&mut self, expected: u8) -> Result<()> {
        self.skip_whitespace();
        if self.input.get(self.index) == Some(&expected) {
            self.index += 1;
            Ok(())
        } else {
            Err(self.error(format!("expected `{}`", char::from(expected))))
        }
    }

    #[inline]
    fn parse_literal(&mut self, literal: &[u8]) -> Result<()> {
        self.skip_whitespace();
        let end = self.index.saturating_add(literal.len());
        if self.input.get(self.index..end) == Some(literal) {
            self.index = end;
            Ok(())
        } else {
            Err(self.error("expected JSON literal"))
        }
    }

    fn enter(&mut self) -> Result<()> {
        if self.depth >= MAX_DEPTH {
            return Err(self.error("recursion limit exceeded"));
        }
        self.depth += 1;
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn parse_string(&mut self) -> Result<Cow<'de, str>> {
        self.skip_whitespace();
        if self.input.get(self.index) != Some(&b'"') {
            return Err(self.error("expected a string"));
        }
        self.index += 1;
        let start = self.index;
        let source = self.utf8_source()?;

        let Some(relative) = find_string_special(&self.input[self.index..]) else {
            return Err(self.error("unterminated string"));
        };
        let special = self.index + relative;
        match self.input[special] {
            b'"' => {
                let raw = &self.input[start..special];
                self.index = special + 1;
                debug_assert_eq!(raw, &source.as_bytes()[start..special]);
                Ok(Cow::Borrowed(&source[start..special]))
            }
            b'\\' => {
                self.index = special;
                self.parse_escaped_string(start)
            }
            _ => {
                self.index = special;
                Err(self.error("control character in string"))
            }
        }
    }

    fn parse_escaped_string(&mut self, start: usize) -> Result<Cow<'de, str>> {
        let source = self.utf8_source()?;
        let prefix = &source[start..self.index];
        let mut output = String::with_capacity(prefix.len() + 8);
        output.push_str(prefix);

        loop {
            match self.input.get(self.index).copied() {
                Some(b'"') => {
                    self.index += 1;
                    return Ok(Cow::Owned(output));
                }
                Some(b'\\') => {
                    self.index += 1;
                    let escaped = self
                        .input
                        .get(self.index)
                        .copied()
                        .ok_or_else(|| self.error("unterminated escape"))?;
                    self.index += 1;
                    match escaped {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{0008}'),
                        b'f' => output.push('\u{000c}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => {
                            let code = self.parse_hex_quad()?;
                            if (0xD800..=0xDBFF).contains(&code) {
                                if self.input.get(self.index..self.index + 2) != Some(b"\\u") {
                                    return Err(self.error("high surrogate without low surrogate"));
                                }
                                self.index += 2;
                                let low = self.parse_hex_quad()?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(self.error("invalid low surrogate"));
                                }
                                let scalar = 0x1_0000
                                    + ((u32::from(code) - 0xD800) << 10)
                                    + (u32::from(low) - 0xDC00);
                                output.push(
                                    char::from_u32(scalar)
                                        .ok_or_else(|| self.error("invalid Unicode scalar"))?,
                                );
                            } else if (0xDC00..=0xDFFF).contains(&code) {
                                return Err(self.error("unexpected low surrogate"));
                            } else {
                                output.push(
                                    char::from_u32(u32::from(code))
                                        .ok_or_else(|| self.error("invalid Unicode scalar"))?,
                                );
                            }
                        }
                        _ => return Err(self.error("invalid string escape")),
                    }
                }
                Some(byte) if byte < 0x20 => {
                    return Err(self.error("control character in string"));
                }
                Some(_) => {
                    let segment_start = self.index;
                    while let Some(&byte) = self.input.get(self.index) {
                        if matches!(byte, b'"' | b'\\') || byte < 0x20 {
                            break;
                        }
                        self.index += 1;
                    }
                    let segment = &source[segment_start..self.index];
                    output.push_str(segment);
                }
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    fn skip_string(&mut self) -> Result<()> {
        self.skip_whitespace();
        if self.input.get(self.index) != Some(&b'"') {
            return Err(self.error("expected a string"));
        }
        self.index += 1;
        self.utf8_source()?;

        loop {
            let Some(relative) = find_string_special(&self.input[self.index..]) else {
                return Err(self.error("unterminated string"));
            };
            let special = self.index + relative;
            match self.input[special] {
                b'"' => {
                    self.index = special + 1;
                    return Ok(());
                }
                b'\\' => {
                    self.index = special + 1;
                    let escaped = self
                        .input
                        .get(self.index)
                        .copied()
                        .ok_or_else(|| self.error("unterminated escape"))?;
                    self.index += 1;
                    match escaped {
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {}
                        b'u' => {
                            let code = self.parse_hex_quad()?;
                            if (0xD800..=0xDBFF).contains(&code) {
                                if self.input.get(self.index..self.index + 2) != Some(b"\\u") {
                                    return Err(self.error("high surrogate without low surrogate"));
                                }
                                self.index += 2;
                                let low = self.parse_hex_quad()?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(self.error("invalid low surrogate"));
                                }
                            } else if (0xDC00..=0xDFFF).contains(&code) {
                                return Err(self.error("unexpected low surrogate"));
                            }
                        }
                        _ => return Err(self.error("invalid string escape")),
                    }
                }
                _ => {
                    self.index = special;
                    return Err(self.error("control character in string"));
                }
            }
        }
    }

    fn skip_value(&mut self) -> Result<()> {
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null"),
            Some(b't') => self.parse_literal(b"true"),
            Some(b'f') => self.parse_literal(b"false"),
            Some(b'"') => self.skip_string(),
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(|_| ()),
            Some(b'[') => self.skip_array(),
            Some(b'{') => self.skip_object(),
            Some(_) => Err(self.error("expected a JSON value")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn skip_array(&mut self) -> Result<()> {
        self.expect_byte(b'[')?;
        self.enter()?;
        let result = (|| {
            if self.peek() == Some(b']') {
                self.index += 1;
                return Ok(());
            }
            loop {
                self.skip_value()?;
                match self.peek() {
                    Some(b',') => self.index += 1,
                    Some(b']') => {
                        self.index += 1;
                        return Ok(());
                    }
                    _ => return Err(self.error("expected `,` or `]`")),
                }
            }
        })();
        self.leave();
        result
    }

    fn skip_object(&mut self) -> Result<()> {
        self.expect_byte(b'{')?;
        self.enter()?;
        let result = (|| {
            if self.peek() == Some(b'}') {
                self.index += 1;
                return Ok(());
            }
            loop {
                self.skip_string()?;
                self.expect_byte(b':')?;
                self.skip_value()?;
                match self.peek() {
                    Some(b',') => self.index += 1,
                    Some(b'}') => {
                        self.index += 1;
                        return Ok(());
                    }
                    _ => return Err(self.error("expected `,` or `}`")),
                }
            }
        })();
        self.leave();
        result
    }

    fn capture_raw(&mut self) -> Result<&'de str> {
        self.skip_whitespace();
        let start = self.index;
        self.skip_value()?;
        str::from_utf8(&self.input[start..self.index])
            .map_err(|_| self.error("input is not valid UTF-8"))
    }

    fn parse_hex_quad(&mut self) -> Result<u16> {
        let end = self.index.saturating_add(4);
        let digits = self
            .input
            .get(self.index..end)
            .ok_or_else(|| self.error("incomplete Unicode escape"))?;
        let mut value = 0_u16;
        for &digit in digits {
            value = value
                .checked_mul(16)
                .and_then(|value| value.checked_add(u16::from(hex_value(digit)?)))
                .ok_or_else(|| self.error("invalid Unicode escape"))?;
        }
        self.index = end;
        Ok(value)
    }

    #[inline]
    fn parse_number(&mut self) -> Result<ParsedNumber<'de>> {
        self.skip_whitespace();
        let start = self.index;
        let negative = self.input.get(self.index) == Some(&b'-');
        if negative {
            self.index += 1;
        }

        let mut magnitude = Some(0_u64);
        match self.input.get(self.index).copied() {
            Some(b'0') => {
                self.index += 1;
                if self.input.get(self.index).is_some_and(u8::is_ascii_digit) {
                    return Err(self.error("leading zero in number"));
                }
            }
            Some(first @ b'1'..=b'9') => {
                magnitude = Some(u64::from(first - b'0'));
                self.index += 1;
                while let Some(digit @ b'0'..=b'9') = self.input.get(self.index).copied() {
                    magnitude = magnitude.and_then(|value| {
                        value
                            .checked_mul(10)
                            .and_then(|value| value.checked_add(u64::from(digit - b'0')))
                    });
                    self.index += 1;
                }
            }
            _ => return Err(self.error("invalid number")),
        }

        let mut float = false;
        if self.input.get(self.index) == Some(&b'.') {
            float = true;
            self.index += 1;
            let fraction_start = self.index;
            while self.input.get(self.index).is_some_and(u8::is_ascii_digit) {
                self.index += 1;
            }
            if self.index == fraction_start {
                return Err(self.error("fraction has no digits"));
            }
        }

        if matches!(self.input.get(self.index), Some(b'e' | b'E')) {
            float = true;
            self.index += 1;
            if matches!(self.input.get(self.index), Some(b'+' | b'-')) {
                self.index += 1;
            }
            let exponent_start = self.index;
            while self.input.get(self.index).is_some_and(u8::is_ascii_digit) {
                self.index += 1;
            }
            if self.index == exponent_start {
                return Err(self.error("exponent has no digits"));
            }
        }

        let raw = str::from_utf8(&self.input[start..self.index])
            .map_err(|_| self.error("number is not ASCII"))?;
        Ok(ParsedNumber {
            raw,
            float,
            negative,
            magnitude,
        })
    }

    #[inline]
    fn parse_u64_value(&mut self) -> Result<u64> {
        self.skip_whitespace();
        let first = self
            .input
            .get(self.index)
            .copied()
            .ok_or_else(|| self.error("invalid number"))?;
        if first == b'-' {
            return Err(self.error("expected an unsigned integer"));
        }

        let mut value = match first {
            b'0' => 0,
            b'1'..=b'9' => u64::from(first - b'0'),
            _ => return Err(self.error("invalid number")),
        };
        self.index += 1;

        if first == b'0' && self.input.get(self.index).is_some_and(u8::is_ascii_digit) {
            return Err(self.error("leading zero in number"));
        }

        while let Some(digit @ b'0'..=b'9') = self.input.get(self.index).copied() {
            value = value
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(digit - b'0')))
                .ok_or_else(|| self.error("integer is out of range"))?;
            self.index += 1;
        }

        if matches!(self.input.get(self.index), Some(b'.' | b'e' | b'E')) {
            return Err(self.error("expected an unsigned integer"));
        }
        Ok(value)
    }

    #[inline]
    fn parse_i64_value(&mut self) -> Result<i64> {
        self.skip_whitespace();
        let negative = self.input.get(self.index) == Some(&b'-');
        if negative {
            self.index += 1;
        }
        let first = self
            .input
            .get(self.index)
            .copied()
            .ok_or_else(|| self.error("invalid number"))?;

        let mut magnitude = match first {
            b'0' => 0,
            b'1'..=b'9' => u64::from(first - b'0'),
            _ => return Err(self.error("invalid number")),
        };
        self.index += 1;

        if first == b'0' && self.input.get(self.index).is_some_and(u8::is_ascii_digit) {
            return Err(self.error("leading zero in number"));
        }
        while let Some(digit @ b'0'..=b'9') = self.input.get(self.index).copied() {
            magnitude = magnitude
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(digit - b'0')))
                .ok_or_else(|| self.error("integer is out of range"))?;
            self.index += 1;
        }
        if matches!(self.input.get(self.index), Some(b'.' | b'e' | b'E')) {
            return Err(self.error("expected an integer"));
        }

        if negative {
            if magnitude == (i64::MAX as u64) + 1 {
                Ok(i64::MIN)
            } else {
                i64::try_from(magnitude)
                    .map(|value| -value)
                    .map_err(|_| self.error("integer is out of range"))
            }
        } else {
            i64::try_from(magnitude).map_err(|_| self.error("integer is out of range"))
        }
    }

    #[inline]
    fn numeric_start(&mut self) -> Result<usize> {
        self.skip_whitespace();
        let start = self.index;
        let unsigned_start = if self.input.get(start) == Some(&b'-') {
            start + 1
        } else {
            start
        };
        let first = self
            .input
            .get(unsigned_start)
            .copied()
            .ok_or_else(|| self.error("invalid number"))?;
        if !first.is_ascii_digit() {
            return Err(self.error("invalid number"));
        }
        if first == b'0'
            && self
                .input
                .get(unsigned_start + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            return Err(self.error("leading zero in number"));
        }
        Ok(start)
    }

    #[inline]
    fn parse_f32_value(&mut self) -> Result<f32> {
        let start = self.numeric_start()?;
        let (value, consumed) =
            lexical_core::parse_partial_with_options::<f32, { lexical_core::format::JSON }>(
                &self.input[start..],
                &lexical_core::parse_float_options::JSON,
            )
            .map_err(|_| self.error("number is out of range"))?;
        self.index = start + consumed;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(self.error("number is out of range"))
        }
    }

    #[inline]
    fn parse_f64_value(&mut self) -> Result<f64> {
        let start = self.numeric_start()?;
        let (value, consumed) =
            lexical_core::parse_partial_with_options::<f64, { lexical_core::format::JSON }>(
                &self.input[start..],
                &lexical_core::parse_float_options::JSON,
            )
            .map_err(|_| self.error("number is out of range"))?;
        self.index = start + consumed;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(self.error("number is out of range"))
        }
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct ParsedNumber<'de> {
    raw: &'de str,
    float: bool,
    negative: bool,
    magnitude: Option<u64>,
}

impl ParsedNumber<'_> {
    fn visit_any<'de, V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if self.float || self.raw == "-0" {
            let value = lexical_core::parse_with_options::<f64, { lexical_core::format::JSON }>(
                self.raw.as_bytes(),
                &lexical_core::parse_float_options::JSON,
            )
            .map_err(|_| Error::message("number is out of range"))?;
            if !value.is_finite() {
                return Err(Error::message("number is out of range"));
            }
            return visitor.visit_f64(value);
        }
        if self.negative {
            self.checked_i64()
                .ok_or_else(|| Error::message("integer is out of range"))
                .and_then(|value| visitor.visit_i64(value))
        } else {
            self.magnitude
                .ok_or_else(|| Error::message("integer is out of range"))
                .and_then(|value| visitor.visit_u64(value))
        }
    }

    #[inline]
    fn checked_i64(&self) -> Option<i64> {
        let magnitude = self.magnitude?;
        if self.negative {
            if magnitude == (i64::MAX as u64) + 1 {
                Some(i64::MIN)
            } else {
                i64::try_from(magnitude).ok().map(|value| -value)
            }
        } else {
            i64::try_from(magnitude).ok()
        }
    }
}

/// Deserializes one complete JSON value from a byte slice.
///
/// # Errors
///
/// Returns an error for malformed JSON, trailing data, or a Serde type mismatch.
#[inline]
pub fn from_slice<'de, T: Deserialize<'de>>(input: &'de [u8]) -> Result<T> {
    let mut deserializer = Deserializer::from_slice(input);
    let value = T::deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

/// Deserializes one complete JSON value from a string.
///
/// # Errors
///
/// Returns an error for malformed JSON, trailing data, or a Serde type mismatch.
#[inline]
pub fn from_str<'de, T: Deserialize<'de>>(input: &'de str) -> Result<T> {
    let mut deserializer = Deserializer::from_str(input);
    let value = T::deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

impl<'de> de::Deserializer<'de> for &mut Deserializer<'de> {
    type Error = Error;

    #[inline]
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.peek() {
            Some(b'n') => {
                self.parse_literal(b"null")?;
                visitor.visit_unit()
            }
            Some(b't') => {
                self.parse_literal(b"true")?;
                visitor.visit_bool(true)
            }
            Some(b'f') => {
                self.parse_literal(b"false")?;
                visitor.visit_bool(false)
            }
            Some(b'"') => match self.parse_string()? {
                Cow::Borrowed(value) => visitor.visit_borrowed_str(value),
                Cow::Owned(value) => visitor.visit_string(value),
            },
            Some(b'[') => self.deserialize_seq(visitor),
            Some(b'{') => self.deserialize_map(visitor),
            Some(b'-' | b'0'..=b'9') => self.parse_number()?.visit_any(visitor),
            Some(_) => Err(self.error("expected a JSON value")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    #[inline]
    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.peek() {
            Some(b't') => {
                self.parse_literal(b"true")?;
                visitor.visit_bool(true)
            }
            Some(b'f') => {
                self.parse_literal(b"false")?;
                visitor.visit_bool(false)
            }
            _ => Err(self.error("expected a boolean")),
        }
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i8(
            i8::try_from(self.parse_i64_value()?)
                .map_err(|_| self.error("integer is out of range"))?,
        )
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i16(
            i16::try_from(self.parse_i64_value()?)
                .map_err(|_| self.error("integer is out of range"))?,
        )
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i32(
            i32::try_from(self.parse_i64_value()?)
                .map_err(|_| self.error("integer is out of range"))?,
        )
    }

    #[inline]
    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i64(self.parse_i64_value()?)
    }

    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i128(self.parse_integer()?)
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u8(
            u8::try_from(self.parse_u64_value()?)
                .map_err(|_| self.error("integer is out of range"))?,
        )
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u16(
            u16::try_from(self.parse_u64_value()?)
                .map_err(|_| self.error("integer is out of range"))?,
        )
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u32(
            u32::try_from(self.parse_u64_value()?)
                .map_err(|_| self.error("integer is out of range"))?,
        )
    }

    #[inline]
    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u64(self.parse_u64_value()?)
    }

    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u128(self.parse_unsigned()?)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_f32(self.parse_f32_value()?)
    }

    #[inline]
    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_f64(self.parse_f64_value()?)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let value = self.parse_string()?;
        let mut characters = value.chars();
        let character = characters
            .next()
            .ok_or_else(|| self.error("expected one character"))?;
        if characters.next().is_some() {
            return Err(self.error("expected one character"));
        }
        visitor.visit_char(character)
    }

    #[inline]
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.parse_string()? {
            Cow::Borrowed(value) => visitor.visit_borrowed_str(value),
            Cow::Owned(value) => visitor.visit_string(value),
        }
    }

    #[inline]
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.parse_string()? {
            Cow::Borrowed(value) => visitor.visit_str(value),
            Cow::Owned(value) => visitor.visit_string(value),
        }
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_seq(visitor)
    }

    #[inline]
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if self.peek() == Some(b'n') {
            self.parse_literal(b"null")?;
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.parse_literal(b"null")?;
        visitor.visit_unit()
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
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        if name == RAW_JSON_TOKEN {
            visitor.visit_borrowed_str(self.capture_raw()?)
        } else {
            visitor.visit_newtype_struct(self)
        }
    }

    #[inline]
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.expect_byte(b'[')?;
        self.enter()?;
        let value = visitor.visit_seq(SliceSeqAccess {
            deserializer: self,
            first: true,
            finished: false,
        });
        if value.is_err() {
            self.leave();
        }
        value
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

    #[inline]
    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.expect_byte(b'{')?;
        self.enter()?;
        let value = visitor.visit_map(SliceMapAccess {
            deserializer: self,
            first: true,
            finished: false,
            value_pending: false,
        });
        if value.is_err() {
            self.leave();
        }
        value
    }

    #[inline]
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
        match self.peek() {
            Some(b'"') => match self.parse_string()? {
                Cow::Borrowed(value) => visitor.visit_enum(value.into_deserializer()),
                Cow::Owned(value) => visitor.visit_enum(value.into_deserializer()),
            },
            Some(b'{') => {
                self.expect_byte(b'{')?;
                self.enter()?;
                visitor.visit_enum(SliceEnumAccess { deserializer: self })
            }
            _ => Err(self.error("expected an enum string or object")),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.skip_value()?;
        visitor.visit_unit()
    }
}

impl Deserializer<'_> {
    fn parse_integer<T: str::FromStr>(&mut self) -> Result<T> {
        let number = self.parse_number()?;
        if number.float {
            return Err(self.error("expected an integer"));
        }
        number
            .raw
            .parse()
            .map_err(|_| self.error("integer is out of range"))
    }

    fn parse_unsigned<T: str::FromStr>(&mut self) -> Result<T> {
        let number = self.parse_number()?;
        if number.float || number.raw.starts_with('-') {
            return Err(self.error("expected an unsigned integer"));
        }
        number
            .raw
            .parse()
            .map_err(|_| self.error("integer is out of range"))
    }
}

struct SliceSeqAccess<'a, 'de> {
    deserializer: &'a mut Deserializer<'de>,
    first: bool,
    finished: bool,
}

impl<'de> SeqAccess<'de> for SliceSeqAccess<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        if self.finished {
            return Ok(None);
        }
        self.deserializer.skip_whitespace();
        if self.deserializer.input.get(self.deserializer.index) == Some(&b']') {
            self.deserializer.index += 1;
            self.deserializer.leave();
            self.finished = true;
            return Ok(None);
        }
        if !self.first {
            self.deserializer.expect_byte(b',')?;
            self.deserializer.skip_whitespace();
            if self.deserializer.input.get(self.deserializer.index) == Some(&b']') {
                return Err(self.deserializer.error("trailing comma in array"));
            }
        }
        self.first = false;
        seed.deserialize(&mut *self.deserializer).map(Some)
    }
}

struct SliceMapAccess<'a, 'de> {
    deserializer: &'a mut Deserializer<'de>,
    first: bool,
    finished: bool,
    value_pending: bool,
}

impl<'de> MapAccess<'de> for SliceMapAccess<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if self.finished {
            return Ok(None);
        }
        if self.value_pending {
            return Err(self.deserializer.error("object value was not consumed"));
        }
        self.deserializer.skip_whitespace();
        if self.deserializer.input.get(self.deserializer.index) == Some(&b'}') {
            self.deserializer.index += 1;
            self.deserializer.leave();
            self.finished = true;
            return Ok(None);
        }
        if !self.first {
            self.deserializer.expect_byte(b',')?;
            self.deserializer.skip_whitespace();
            if self.deserializer.input.get(self.deserializer.index) == Some(&b'}') {
                return Err(self.deserializer.error("trailing comma in object"));
            }
        }
        self.first = false;
        let key = self.deserializer.parse_string()?;
        self.deserializer.expect_byte(b':')?;
        self.value_pending = true;
        seed.deserialize(KeyDeserializer { key }).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        if !self.value_pending {
            return Err(self.deserializer.error("object key was not consumed"));
        }
        self.value_pending = false;
        seed.deserialize(&mut *self.deserializer)
    }
}

struct KeyDeserializer<'de> {
    key: Cow<'de, str>,
}

macro_rules! deserialize_key_number {
    ($method:ident, $visit:ident, $type:ty) => {
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
            let value = self
                .key
                .parse::<$type>()
                .map_err(|_| Error::message("object key is not the requested number"))?;
            visitor.$visit(value)
        }
    };
}

impl<'de> de::Deserializer<'de> for KeyDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.key {
            Cow::Borrowed(key) => visitor.visit_borrowed_str(key),
            Cow::Owned(key) => visitor.visit_string(key),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.key.as_ref() {
            "true" => visitor.visit_bool(true),
            "false" => visitor.visit_bool(false),
            _ => Err(Error::message("object key is not a boolean")),
        }
    }

    deserialize_key_number!(deserialize_i8, visit_i8, i8);
    deserialize_key_number!(deserialize_i16, visit_i16, i16);
    deserialize_key_number!(deserialize_i32, visit_i32, i32);
    deserialize_key_number!(deserialize_i64, visit_i64, i64);
    deserialize_key_number!(deserialize_i128, visit_i128, i128);
    deserialize_key_number!(deserialize_u8, visit_u8, u8);
    deserialize_key_number!(deserialize_u16, visit_u16, u16);
    deserialize_key_number!(deserialize_u32, visit_u32, u32);
    deserialize_key_number!(deserialize_u64, visit_u64, u64);
    deserialize_key_number!(deserialize_u128, visit_u128, u128);

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let mut characters = self.key.chars();
        let value = characters
            .next()
            .ok_or_else(|| Error::message("object key is not a character"))?;
        if characters.next().is_some() {
            return Err(Error::message("object key is not a character"));
        }
        visitor.visit_char(value)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_any(visitor)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_any(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_enum(self.key.into_deserializer())
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_any(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_any(visitor)
    }

    serde::forward_to_deserialize_any! {
        f32 f64 bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map struct
    }
}

struct SliceEnumAccess<'a, 'de> {
    deserializer: &'a mut Deserializer<'de>,
}

impl<'de, 'a> EnumAccess<'de> for SliceEnumAccess<'a, 'de> {
    type Error = Error;
    type Variant = SliceVariantAccess<'a, 'de>;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant)> {
        let key = self.deserializer.parse_string()?;
        self.deserializer.expect_byte(b':')?;
        let value = seed.deserialize(KeyDeserializer { key })?;
        Ok((
            value,
            SliceVariantAccess {
                deserializer: self.deserializer,
            },
        ))
    }
}

struct SliceVariantAccess<'a, 'de> {
    deserializer: &'a mut Deserializer<'de>,
}

impl SliceVariantAccess<'_, '_> {
    fn finish(self) -> Result<()> {
        self.deserializer.expect_byte(b'}')?;
        self.deserializer.leave();
        Ok(())
    }
}

impl<'de> VariantAccess<'de> for SliceVariantAccess<'_, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        self.deserializer.parse_literal(b"null")?;
        self.finish()
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        let value = seed.deserialize(&mut *self.deserializer)?;
        self.finish()?;
        Ok(value)
    }

    fn tuple_variant<V: Visitor<'de>>(self, length: usize, visitor: V) -> Result<V::Value> {
        let value = de::Deserializer::deserialize_tuple(&mut *self.deserializer, length, visitor)?;
        self.finish()?;
        Ok(value)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        let value =
            de::Deserializer::deserialize_struct(&mut *self.deserializer, "", fields, visitor)?;
        self.finish()?;
        Ok(value)
    }
}

impl fmt::Debug for Deserializer<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Deserializer")
            .field("index", &self.index)
            .field("remaining", &self.input.len().saturating_sub(self.index))
            .finish()
    }
}
