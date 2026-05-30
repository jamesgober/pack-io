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

/// Maximum number of bytes a `u64` ever occupies as LEB128 (10).
pub(crate) const MAX_VARINT_LEN_U64: usize = 10;
/// Maximum number of bytes a `u128` ever occupies as LEB128 (19).
pub(crate) const MAX_VARINT_LEN_U128: usize = 19;

/// Write `value` into `out` as an unsigned LEB128 varint, returning the
/// number of bytes consumed.
///
/// `out` MUST be at least [`MAX_VARINT_LEN_U64`] bytes long. Internal use:
/// callers in this crate pass a stack-allocated `[u8; MAX_VARINT_LEN_U64]`.
#[inline]
pub(crate) fn write_u64(value: u64, out: &mut [u8]) -> usize {
    let mut n = value;
    let mut written = 0usize;
    while n >= 0x80 {
        out[written] = (n as u8) | 0x80;
        n >>= 7;
        written += 1;
    }
    out[written] = n as u8;
    written + 1
}

/// Write `value` into `out` as an unsigned LEB128 varint, returning the
/// number of bytes consumed.
///
/// `out` MUST be at least [`MAX_VARINT_LEN_U128`] bytes long.
#[inline]
pub(crate) fn write_u128(value: u128, out: &mut [u8]) -> usize {
    let mut n = value;
    let mut written = 0usize;
    while n >= 0x80 {
        out[written] = (n as u8) | 0x80;
        n >>= 7;
        written += 1;
    }
    out[written] = n as u8;
    written + 1
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
    use crate::codec::{Decode, Decoder, Encode, Encoder};

    #[test]
    fn write_zero_is_one_byte() {
        let mut buf = [0u8; MAX_VARINT_LEN_U64];
        let n = write_u64(0, &mut buf);
        assert_eq!(n, 1);
        assert_eq!(buf[0], 0x00);
    }

    #[test]
    fn write_one_twenty_seven_is_one_byte() {
        let mut buf = [0u8; MAX_VARINT_LEN_U64];
        let n = write_u64(127, &mut buf);
        assert_eq!(n, 1);
        assert_eq!(buf[0], 0x7f);
    }

    #[test]
    fn write_one_twenty_eight_is_two_bytes() {
        let mut buf = [0u8; MAX_VARINT_LEN_U64];
        let n = write_u64(128, &mut buf);
        assert_eq!(n, 2);
        assert_eq!(&buf[..2], &[0x80, 0x01]);
    }

    #[test]
    fn write_u64_max_is_ten_bytes() {
        let mut buf = [0u8; MAX_VARINT_LEN_U64];
        let n = write_u64(u64::MAX, &mut buf);
        assert_eq!(n, MAX_VARINT_LEN_U64);
    }

    #[test]
    fn varint_round_trips_via_codec() {
        for v in [0u64, 1, 2, 127, 128, 255, 256, 16383, 16384, u64::MAX] {
            let mut enc = Encoder::new();
            enc.write_varint_u64(v).unwrap();
            let bytes = enc.into_inner();
            let mut dec = Decoder::new(&bytes);
            let decoded = dec.read_varint_u64().unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn varint_u128_round_trips_via_codec() {
        for v in [
            0u128,
            1,
            127,
            128,
            (1u128 << 64) - 1,
            1u128 << 64,
            u128::MAX,
        ] {
            let mut enc = Encoder::new();
            enc.write_varint_u128(v).unwrap();
            let bytes = enc.into_inner();
            let mut dec = Decoder::new(&bytes);
            let decoded = dec.read_varint_u128().unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn decoder_rejects_overlong_varint() {
        let bytes = [0xffu8; 11];
        let mut dec = Decoder::new(&bytes);
        let err = dec.read_varint_u64().expect_err("overlong varint");
        assert!(matches!(err, crate::SerialError::VarintOverflow));
    }

    #[test]
    fn decoder_rejects_tenth_byte_with_high_bits() {
        let mut bytes = [0xffu8; 10];
        bytes[9] = 0x02;
        let mut dec = Decoder::new(&bytes);
        let err = dec.read_varint_u64().expect_err("u64 overflow");
        assert!(matches!(err, crate::SerialError::VarintOverflow));
    }

    #[test]
    fn decoder_rejects_truncated_varint() {
        let mut dec = Decoder::new(&[0x80]);
        let err = dec.read_varint_u64().expect_err("truncated");
        assert!(matches!(err, crate::SerialError::UnexpectedEof { .. }));
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
    fn zigzag_i128_round_trip() {
        for v in [0i128, -1, 1, i128::MIN, i128::MAX, -(1i128 << 100)] {
            let back = zigzag_decode_i128(zigzag_encode_i128(v));
            assert_eq!(back, v);
        }
    }
}
