// These tiny methods are a cross-crate code-generation primitive. Benchmarks
// show a material protocol-path regression when LLVM does not inline them.
#![allow(clippy::inline_always)]

/// A zero-allocation recognizer for a known canonical JSON layout.
///
/// This is not a general JSON parser. It is an optimization primitive for
/// generated or protocol-specific code that first attempts one exact layout
/// and falls back to [`crate::Cursor`] when any expectation does not match.
/// Every successful recognizer must consume and check the complete input.
#[derive(Clone, Copy, Debug)]
pub struct CanonicalScanner<'a> {
    remaining: &'a str,
}

impl<'a> CanonicalScanner<'a> {
    /// Starts matching an already UTF-8-validated string.
    #[must_use]
    #[inline(always)]
    pub const fn new(input: &'a str) -> Self {
        Self { remaining: input }
    }

    /// Starts matching a byte slice after validating UTF-8 once.
    #[must_use]
    #[inline(always)]
    pub fn from_slice(input: &'a [u8]) -> Option<Self> {
        std::str::from_utf8(input).ok().map(Self::new)
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
