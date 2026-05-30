//! LEB128 varint encoding and ZigZag mapping for signed integers.
//!
//! All multi-byte unsigned integers are encoded as **unsigned LEB128**: a
//! little-endian base-128 encoding where each byte carries 7 bits of payload
//! and a continuation bit in the most-significant position. Signed integers
//! are first mapped through **ZigZag** so that small-magnitude negative
//! values still encode in a single byte.
//!
//! This is the same wire shape used by `protobuf`, `postcard`, `bincode`'s
//! varint mode, and several others. The choice is deliberate: keeping the
//! varint shape conventional means a third-party decoder can read pack-io
//! integers without consulting our source.
//!
//! ## Determinism
//!
//! For a given integer value, the LEB128 encoding is **unique**: there are
//! no redundant trailing-zero forms (encoders MUST stop on the first byte
//! whose continuation bit is zero). Decoders enforce this by rejecting any
//! varint that exceeds the legal byte count for its target width.

use crate::error::{Result, SerialError};

/// Maximum number of bytes a `u64` ever occupies as LEB128 (10).
pub(crate) const MAX_VARINT_LEN_U64: usize = 10;
/// Maximum number of bytes a `u128` ever occupies as LEB128 (19).
pub(crate) const MAX_VARINT_LEN_U128: usize = 19;

/// Encode a `u64` into `out` as an unsigned LEB128 varint.
///
/// Pushes between 1 and [`MAX_VARINT_LEN_U64`] bytes. Returns the number of
/// bytes appended so callers that build framed records can avoid re-reading
/// the buffer length.
#[inline]
pub(crate) fn encode_u64(value: u64, out: &mut alloc::vec::Vec<u8>) -> usize {
    let mut n = value;
    let mut written = 0usize;
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
        written += 1;
    }
    out.push(n as u8);
    written + 1
}

/// Encode a `u128` into `out` as an unsigned LEB128 varint.
///
/// Used for the 128-bit integer types. Up to [`MAX_VARINT_LEN_U128`] bytes.
#[inline]
pub(crate) fn encode_u128(value: u128, out: &mut alloc::vec::Vec<u8>) -> usize {
    let mut n = value;
    let mut written = 0usize;
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
        written += 1;
    }
    out.push(n as u8);
    written + 1
}

/// Decode an unsigned LEB128 varint into a `u64`.
///
/// `bytes` is the read cursor; on success the cursor is advanced past the
/// consumed prefix and the decoded value is returned. The decoder rejects:
///
/// - Truncated input (returns [`SerialError::UnexpectedEof`]).
/// - Varints longer than [`MAX_VARINT_LEN_U64`] bytes (returns
///   [`SerialError::VarintOverflow`]).
/// - The 10th byte having any bits set above bit 0 — that would overflow
///   `u64` (also [`SerialError::VarintOverflow`]).
#[inline]
pub(crate) fn decode_u64(bytes: &[u8]) -> Result<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut consumed = 0usize;
    while consumed < MAX_VARINT_LEN_U64 {
        let byte = match bytes.get(consumed) {
            Some(b) => *b,
            None => {
                return Err(SerialError::UnexpectedEof {
                    needed: 1,
                    remaining: 0,
                });
            }
        };
        consumed += 1;

        // The 10th byte (consumed == 10 after this loop iteration's increment)
        // may only set bit 0 — anything else overflows u64.
        if consumed == MAX_VARINT_LEN_U64 && (byte & 0xfe) != 0 {
            return Err(SerialError::VarintOverflow);
        }

        let low7 = u64::from(byte & 0x7f);
        result |= low7 << shift;

        if byte & 0x80 == 0 {
            return Ok((result, consumed));
        }
        shift += 7;
    }

    Err(SerialError::VarintOverflow)
}

/// Decode an unsigned LEB128 varint into a `u128`.
#[inline]
pub(crate) fn decode_u128(bytes: &[u8]) -> Result<(u128, usize)> {
    let mut result: u128 = 0;
    let mut shift: u32 = 0;
    let mut consumed = 0usize;
    while consumed < MAX_VARINT_LEN_U128 {
        let byte = match bytes.get(consumed) {
            Some(b) => *b,
            None => {
                return Err(SerialError::UnexpectedEof {
                    needed: 1,
                    remaining: 0,
                });
            }
        };
        consumed += 1;

        // The 19th byte may only set the low bit (and nothing else) — u128 is
        // 128 bits, and 18 * 7 = 126, so the last byte holds the top 2 bits.
        // Bits 2..=6 of byte 19 are required to be zero. Bit 7 must also be
        // zero (no continuation past byte 19).
        if consumed == MAX_VARINT_LEN_U128 && (byte & 0xfc) != 0 {
            return Err(SerialError::VarintOverflow);
        }

        let low7 = u128::from(byte & 0x7f);
        result |= low7 << shift;

        if byte & 0x80 == 0 {
            return Ok((result, consumed));
        }
        shift += 7;
    }

    Err(SerialError::VarintOverflow)
}

/// ZigZag-encode a signed integer into an unsigned one of the same width.
///
/// ZigZag maps `0, -1, 1, -2, 2, …` to `0, 1, 2, 3, 4, …` so the absolute
/// magnitude determines the resulting LEB128 size — not the two's-complement
/// representation, which would force every negative integer to the maximum
/// 10 bytes.
#[inline]
pub(crate) const fn zigzag_encode_i64(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

/// Inverse of [`zigzag_encode_i64`].
#[inline]
pub(crate) const fn zigzag_decode_i64(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

/// ZigZag for `i128`.
#[inline]
pub(crate) const fn zigzag_encode_i128(value: i128) -> u128 {
    ((value << 1) ^ (value >> 127)) as u128
}

/// Inverse of [`zigzag_encode_i128`].
#[inline]
pub(crate) const fn zigzag_decode_i128(value: u128) -> i128 {
    ((value >> 1) as i128) ^ -((value & 1) as i128)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn encode_zero_is_one_byte() {
        let mut buf = Vec::new();
        let n = encode_u64(0, &mut buf);
        assert_eq!(n, 1);
        assert_eq!(buf, [0x00]);
    }

    #[test]
    fn encode_one_twenty_seven_is_one_byte() {
        let mut buf = Vec::new();
        let n = encode_u64(127, &mut buf);
        assert_eq!(n, 1);
        assert_eq!(buf, [0x7f]);
    }

    #[test]
    fn encode_one_twenty_eight_is_two_bytes() {
        let mut buf = Vec::new();
        let n = encode_u64(128, &mut buf);
        assert_eq!(n, 2);
        assert_eq!(buf, [0x80, 0x01]);
    }

    #[test]
    fn encode_u64_max_is_ten_bytes() {
        let mut buf = Vec::new();
        let n = encode_u64(u64::MAX, &mut buf);
        assert_eq!(n, MAX_VARINT_LEN_U64);
        assert_eq!(buf.len(), MAX_VARINT_LEN_U64);
    }

    #[test]
    fn decode_round_trip_small_values() {
        let mut buf = Vec::new();
        for v in [0u64, 1, 2, 127, 128, 255, 256, 16383, 16384, u64::MAX] {
            buf.clear();
            let _ = encode_u64(v, &mut buf);
            let (decoded, consumed) = decode_u64(&buf).expect("varint decodes");
            assert_eq!(decoded, v);
            assert_eq!(consumed, buf.len());
        }
    }

    #[test]
    fn decode_truncated_returns_unexpected_eof() {
        let result = decode_u64(&[0x80]);
        assert!(matches!(result, Err(SerialError::UnexpectedEof { .. })));
    }

    #[test]
    fn decode_empty_returns_unexpected_eof() {
        let result = decode_u64(&[]);
        assert!(matches!(result, Err(SerialError::UnexpectedEof { .. })));
    }

    #[test]
    fn decode_overlong_varint_is_rejected() {
        // 11 continuation bytes - clearly too long for u64.
        let bad = [0xff; 11];
        let result = decode_u64(&bad);
        assert!(matches!(result, Err(SerialError::VarintOverflow)));
    }

    #[test]
    fn decode_tenth_byte_with_high_bits_is_rejected() {
        // Valid 9 continuation bytes, then a 10th byte that would overflow u64.
        let mut bad = [0xff; 10];
        bad[9] = 0x02; // bit 1 set on byte 10 - overflows u64
        let result = decode_u64(&bad);
        assert!(matches!(result, Err(SerialError::VarintOverflow)));
    }

    #[test]
    fn zigzag_round_trip_for_signed_values() {
        for v in [
            0i64,
            -1,
            1,
            -2,
            2,
            i64::MIN,
            i64::MAX,
            i64::MIN + 1,
            i64::MAX - 1,
        ] {
            let z = zigzag_encode_i64(v);
            let back = zigzag_decode_i64(z);
            assert_eq!(back, v, "round trip failed for {v}");
        }
    }

    #[test]
    fn zigzag_maps_minus_one_to_one() {
        assert_eq!(zigzag_encode_i64(-1), 1);
        assert_eq!(zigzag_encode_i64(1), 2);
        assert_eq!(zigzag_encode_i64(-2), 3);
    }

    #[test]
    fn encode_decode_u128_round_trip() {
        let mut buf = Vec::new();
        for v in [
            0u128,
            1,
            127,
            128,
            (1u128 << 64) - 1,
            1u128 << 64,
            u128::MAX,
        ] {
            buf.clear();
            let _ = encode_u128(v, &mut buf);
            let (decoded, _) = decode_u128(&buf).expect("varint decodes");
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn decode_u128_rejects_overlong() {
        let bad = [0xff; 20];
        assert!(matches!(
            decode_u128(&bad),
            Err(SerialError::VarintOverflow)
        ));
    }

    #[test]
    fn zigzag_i128_round_trip() {
        for v in [0i128, -1, 1, i128::MIN, i128::MAX, -(1i128 << 100)] {
            let back = zigzag_decode_i128(zigzag_encode_i128(v));
            assert_eq!(back, v);
        }
    }
}
