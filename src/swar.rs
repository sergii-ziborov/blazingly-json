//! Word-at-a-time scanning shared by the parser and the writer.
//!
//! Both directions ask the same question of a JSON string — where is the first
//! byte that is not ordinary text — so both should ask it the same way. The
//! parser has scanned eight bytes at a time since it was written; the writer
//! walked one byte at a time through a 256-entry table until this module
//! existed to be shared.

const WORD_BYTES: usize = size_of::<u64>();
const ONE_BYTES: u64 = u64::MAX / 255;
const HIGH_BYTES: u64 = ONE_BYTES << 7;

/// The offset of the first byte a JSON string cannot carry literally.
///
/// That is a control byte below `0x20`, a `"`, or a `\` — exactly the set
/// `ESCAPE` marks on the writing side and the set the parser stops on.
///
/// Only the FIRST such offset is reported, via `trailing_zeros`. That matters:
/// the per-byte lanes of the control comparison are not independent, because a
/// borrow out of one lane can disturb the lane above it, so the individual mask
/// bits are candidates rather than answers. Taking the lowest set bit is sound
/// because every lane below it is known clean, and a caller that resumed from
/// `offset + 1` re-derives the next answer from scratch.
#[inline]
pub(crate) fn find_string_special(bytes: &[u8]) -> Option<usize> {
    // Below one word there is nothing to widen, and setting up the chunk
    // iterator costs more than the scan it would replace — measured as a 25%
    // regression on four-byte strings before this guard existed.
    if bytes.len() < WORD_BYTES {
        return scalar_scan(bytes);
    }

    let mut offset = 0;
    let mut chunks = bytes.chunks_exact(WORD_BYTES);

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
        offset += WORD_BYTES;
    }

    scalar_scan(chunks.remainder()).map(|relative| offset + relative)
}

#[inline]
fn scalar_scan(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .position(|&byte| byte < 0x20 || matches!(byte, b'"' | b'\\'))
}

#[cfg(test)]
mod tests {
    use super::find_string_special;

    /// The scalar definition the word-at-a-time version has to agree with.
    fn scalar(bytes: &[u8]) -> Option<usize> {
        bytes
            .iter()
            .position(|&byte| byte < 0x20 || matches!(byte, b'"' | b'\\'))
    }

    #[test]
    fn it_agrees_with_the_scalar_scan_at_every_length_and_position() {
        // Every offset in and past one word, so the chunked loop, the boundary,
        // and the remainder are each exercised with the special byte first,
        // last, and in the middle.
        for length in 0..40_usize {
            for position in 0..length {
                for special in [0x00_u8, 0x01, 0x1f, b'"', b'\\'] {
                    let mut bytes = vec![b'a'; length];
                    bytes[position] = special;
                    assert_eq!(
                        find_string_special(&bytes),
                        scalar(&bytes),
                        "length {length}, position {position}, byte {special:#04x}"
                    );
                }
            }
            assert_eq!(find_string_special(&vec![b'a'; length]), None);
        }
    }

    #[test]
    fn a_high_byte_is_not_special_and_does_not_disturb_its_neighbours() {
        // The borrow in the control-byte comparison propagates upward, so a
        // multi-byte character next to a clean byte is where a naive mask reads
        // a false positive. Every lane of a word is checked against a 0x80-0xff
        // byte, which is what UTF-8 continuation bytes are.
        for high in [0x80_u8, 0xc3, 0xff] {
            for position in 0..16_usize {
                let mut bytes = vec![b'a'; 16];
                bytes[position] = high;
                assert_eq!(
                    find_string_special(&bytes),
                    None,
                    "{high:#04x} @ {position}"
                );

                // And with a real escape after it: the high byte must not hide
                // it or shift its reported offset.
                let mut bytes = vec![b'a'; 16];
                bytes[position] = high;
                bytes[15] = b'"';
                assert_eq!(find_string_special(&bytes), Some(15));
            }
        }
    }

    #[test]
    fn every_byte_value_classifies_the_same_as_the_scalar_scan() {
        for byte in 0..=u8::MAX {
            let bytes = [byte];
            assert_eq!(find_string_special(&bytes), scalar(&bytes), "{byte:#04x}");
        }
    }
}
