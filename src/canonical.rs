// These tiny methods are a cross-crate code-generation primitive. Benchmarks
// show a material protocol-path regression when LLVM does not inline them.
#![allow(clippy::inline_always)]

/// A zero-allocation recognizer for a known canonical JSON layout.
///
/// This is not a general JSON parser. It is an optimization primitive for
/// generated or protocol-specific code that first attempts one exact layout
/// and falls back to [`crate::JsonCursor`] when any expectation does not match.
/// Every successful recognizer must consume and check the complete input.
#[derive(Clone, Copy, Debug)]
pub struct CanonicalScanner<'a> {
    remaining: &'a str,
}

/// A zero-allocation canonical-layout recognizer over raw bytes.
///
/// Unlike converting the complete input with [`std::str::from_utf8`] first,
/// this scanner validates only captured string fields. Structural fragments
/// are matched as ASCII bytes. A successful recognizer still proves that the
/// complete accepted layout is valid UTF-8 JSON because every byte is either
/// an exact ASCII literal, an ASCII number/boolean, or part of a validated
/// string.
#[derive(Clone, Copy, Debug)]
pub struct CanonicalBytesScanner<'a> {
    remaining: &'a [u8],
}

impl<'a> CanonicalScanner<'a> {
    /// Starts matching an already UTF-8-validated string.
    #[must_use]
    #[inline(always)]
    pub const fn new(input: &'a str) -> Self {
        Self { remaining: input }
    }

    /// Consumes an exact string fragment.
    ///
    /// Returns `None` without advancing when the sequence does not match.
    #[inline(always)]
    pub fn literal(&mut self, expected: &str) -> Option<()> {
        self.remaining = self.remaining.strip_prefix(expected)?;
        Some(())
    }

    /// Consumes a JSON string with no escapes and returns its borrowed value.
    ///
    /// Escaped strings deliberately return `None` so the caller can use the
    /// general parser instead.
    #[inline(always)]
    pub fn plain_string(&mut self) -> Option<&'a str> {
        let input = self.remaining.strip_prefix('"')?;
        let end = memchr::memchr(b'"', input.as_bytes())?;
        let value = &input[..end];
        if value
            .as_bytes()
            .iter()
            .any(|byte| *byte == b'\\' || *byte < 0x20)
        {
            return None;
        }
        self.remaining = &input[end + 1..];
        Some(value)
    }

    /// Consumes one unsigned JSON integer.
    #[inline(always)]
    pub fn unsigned(&mut self) -> Option<u64> {
        let bytes = self.remaining.as_bytes();
        let first = *bytes.first()?;
        if !first.is_ascii_digit() {
            return None;
        }
        if first == b'0' && bytes.get(1).is_some_and(u8::is_ascii_digit) {
            return None;
        }

        let mut value = 0_u64;
        let mut length = 0;
        for &digit in bytes {
            if !digit.is_ascii_digit() {
                break;
            }
            value = value
                .checked_mul(10)?
                .checked_add(u64::from(digit - b'0'))?;
            length += 1;
        }
        self.remaining = &self.remaining[length..];
        Some(value)
    }

    /// Consumes `true` or `false`.
    #[inline(always)]
    pub fn boolean(&mut self) -> Option<bool> {
        if let Some(remaining) = self.remaining.strip_prefix("true") {
            self.remaining = remaining;
            Some(true)
        } else if let Some(remaining) = self.remaining.strip_prefix("false") {
            self.remaining = remaining;
            Some(false)
        } else {
            None
        }
    }

    /// Returns the unmatched suffix.
    #[must_use]
    #[inline(always)]
    pub const fn remaining(&self) -> &'a str {
        self.remaining
    }

    /// Returns true only when the recognizer consumed the complete input.
    #[must_use]
    #[inline(always)]
    pub const fn is_finished(&self) -> bool {
        self.remaining.is_empty()
    }
}

impl<'a> CanonicalBytesScanner<'a> {
    /// Starts matching a byte slice without a full-input UTF-8 pre-pass.
    #[must_use]
    #[inline(always)]
    pub const fn new(input: &'a [u8]) -> Self {
        Self { remaining: input }
    }

    /// Consumes an exact ASCII JSON fragment.
    #[inline(always)]
    pub fn literal(&mut self, expected: &str) -> Option<()> {
        self.remaining = self.remaining.strip_prefix(expected.as_bytes())?;
        Some(())
    }

    /// Consumes a JSON string with no escapes and validates only its payload.
    #[inline(always)]
    pub fn plain_string(&mut self) -> Option<&'a str> {
        let input = self.remaining.strip_prefix(b"\"")?;
        let end = memchr::memchr(b'"', input)?;
        let value = &input[..end];
        if value.iter().any(|byte| *byte == b'\\' || *byte < 0x20) {
            return None;
        }
        let value = std::str::from_utf8(value).ok()?;
        self.remaining = &input[end + 1..];
        Some(value)
    }

    /// Consumes an unescaped ASCII JSON string and returns its bytes.
    ///
    /// This is the fastest byte-input path for protocol identifiers and other
    /// fields whose canonical form is ASCII. Non-ASCII input returns `None` so
    /// the caller can fall back to [`Self::plain_string`] or a general parser.
    #[inline(always)]
    pub fn plain_ascii_string(&mut self) -> Option<&'a [u8]> {
        let input = self.remaining.strip_prefix(b"\"")?;
        let end = memchr::memchr(b'"', input)?;
        let value = &input[..end];
        if value
            .iter()
            .any(|byte| !byte.is_ascii() || *byte == b'\\' || *byte < 0x20)
        {
            return None;
        }
        self.remaining = &input[end + 1..];
        Some(value)
    }

    /// Consumes one unsigned JSON integer.
    #[inline(always)]
    pub fn unsigned(&mut self) -> Option<u64> {
        let first = *self.remaining.first()?;
        if !first.is_ascii_digit() {
            return None;
        }
        if first == b'0' && self.remaining.get(1).is_some_and(u8::is_ascii_digit) {
            return None;
        }

        let mut value = 0_u64;
        let mut length = 0;
        for &digit in self.remaining {
            if !digit.is_ascii_digit() {
                break;
            }
            value = value
                .checked_mul(10)?
                .checked_add(u64::from(digit - b'0'))?;
            length += 1;
        }
        self.remaining = &self.remaining[length..];
        Some(value)
    }

    /// Consumes `true` or `false`.
    #[inline(always)]
    pub fn boolean(&mut self) -> Option<bool> {
        if let Some(remaining) = self.remaining.strip_prefix(b"true") {
            self.remaining = remaining;
            Some(true)
        } else if let Some(remaining) = self.remaining.strip_prefix(b"false") {
            self.remaining = remaining;
            Some(false)
        } else {
            None
        }
    }

    /// Returns the unmatched suffix.
    #[must_use]
    #[inline(always)]
    pub const fn remaining(&self) -> &'a [u8] {
        self.remaining
    }

    /// Returns true only when the recognizer consumed the complete input.
    #[must_use]
    #[inline(always)]
    pub const fn is_finished(&self) -> bool {
        self.remaining.is_empty()
    }
}
