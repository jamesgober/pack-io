//! `Serialize` / `Deserialize` implementations for primitive and core types.
//!
//! ## Wire format
//!
//! - `u8` / `i8` — one byte each (fixed). `i8` is two's-complement.
//! - `u16` / `u32` / `u64` / `u128` / `usize` — LEB128 varint. `usize` is
//!   encoded through `u64`; on a 32-bit target a decoded value outside
//!   `usize::MAX` would be rejected with [`SerialError::IntegerOutOfRange`].
//! - `i16` / `i32` / `i64` / `i128` / `isize` — ZigZag mapping followed by
//!   LEB128 varint.
//! - `bool` — one byte (`0x00` / `0x01`); any other byte is rejected.
//! - `f32` / `f64` — IEEE 754 bit pattern, little-endian. NaN is preserved
//!   bit-for-bit; consumers that care about IEEE equality should compare
//!   floats via `to_bits()`.
//! - `String` / `&str` — varint length prefix, then UTF-8 bytes.
//! - `Vec<u8>` / `&[u8]` — varint length prefix, then raw bytes.
//! - `[T; N]` — `N` consecutive `T` encodings, no length prefix (the length
//!   is in the type).
//! - tuples (arity 2..=12) — fields concatenated in declaration order.
//! - `Option<T>` — one tag byte (`0x00` = `None`, `0x01` = `Some`) followed
//!   by the inner value when present.
//! - `Result<T, E>` — one tag byte (`0x00` = `Ok`, `0x01` = `Err`) followed
//!   by the inner value.
//! - `()` (unit) — zero bytes.
//!
//! This wire shape is the same one a third-party implementer would arrive at
//! after reading a one-page spec. The normative spec lands in `0.3` when
//! the format freezes.

use alloc::string::String;
use alloc::vec::Vec;

use crate::codec::{Decoder, Encoder};
use crate::error::{Result, SerialError};
use crate::traits::{Deserialize, Serialize};
use crate::varint;

// ---------------------------------------------------------------------------
// Unsigned integers
// ---------------------------------------------------------------------------

impl Serialize for u8 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.push_byte(*self);
        Ok(())
    }
}

impl Deserialize for u8 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.read_byte()
    }
}

impl Serialize for u16 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u64(u64::from(*self));
        Ok(())
    }
}

impl Deserialize for u16 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        let value = decoder.read_varint_u64()?;
        u16::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

impl Serialize for u32 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u64(u64::from(*self));
        Ok(())
    }
}

impl Deserialize for u32 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        let value = decoder.read_varint_u64()?;
        u32::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

impl Serialize for u64 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u64(*self);
        Ok(())
    }
}

impl Deserialize for u64 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.read_varint_u64()
    }
}

impl Serialize for u128 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u128(*self);
        Ok(())
    }
}

impl Deserialize for u128 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        decoder.read_varint_u128()
    }
}

impl Serialize for usize {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u64(*self as u64);
        Ok(())
    }
}

impl Deserialize for usize {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        let value = decoder.read_varint_u64()?;
        usize::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

// ---------------------------------------------------------------------------
// Signed integers — ZigZag + varint
// ---------------------------------------------------------------------------

impl Serialize for i8 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.push_byte(*self as u8);
        Ok(())
    }
}

impl Deserialize for i8 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(decoder.read_byte()? as i8)
    }
}

impl Serialize for i16 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u64(varint::zigzag_encode_i64(i64::from(*self)));
        Ok(())
    }
}

impl Deserialize for i16 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        let value = varint::zigzag_decode_i64(decoder.read_varint_u64()?);
        i16::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

impl Serialize for i32 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u64(varint::zigzag_encode_i64(i64::from(*self)));
        Ok(())
    }
}

impl Deserialize for i32 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        let value = varint::zigzag_decode_i64(decoder.read_varint_u64()?);
        i32::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

impl Serialize for i64 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u64(varint::zigzag_encode_i64(*self));
        Ok(())
    }
}

impl Deserialize for i64 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(varint::zigzag_decode_i64(decoder.read_varint_u64()?))
    }
}

impl Serialize for i128 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u128(varint::zigzag_encode_i128(*self));
        Ok(())
    }
}

impl Deserialize for i128 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(varint::zigzag_decode_i128(decoder.read_varint_u128()?))
    }
}

impl Serialize for isize {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u64(varint::zigzag_encode_i64(*self as i64));
        Ok(())
    }
}

impl Deserialize for isize {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        let value = varint::zigzag_decode_i64(decoder.read_varint_u64()?);
        isize::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

// ---------------------------------------------------------------------------
// Bool
// ---------------------------------------------------------------------------

impl Serialize for bool {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.push_byte(u8::from(*self));
        Ok(())
    }
}

impl Deserialize for bool {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.read_byte()? {
            0x00 => Ok(false),
            0x01 => Ok(true),
            other => Err(SerialError::InvalidBool { byte: other }),
        }
    }
}

// ---------------------------------------------------------------------------
// Floats — IEEE 754 bit pattern, little-endian
// ---------------------------------------------------------------------------

impl Serialize for f32 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.push_bytes(&self.to_bits().to_le_bytes());
        Ok(())
    }
}

impl Deserialize for f32 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        let bytes = decoder.read_slice(4)?;
        // SAFETY-ADJACENT: read_slice guarantees exactly 4 bytes; copy into a
        // sized array so `u32::from_le_bytes` infers the right shape.
        let mut arr = [0u8; 4];
        arr.copy_from_slice(bytes);
        Ok(f32::from_bits(u32::from_le_bytes(arr)))
    }
}

impl Serialize for f64 {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.push_bytes(&self.to_bits().to_le_bytes());
        Ok(())
    }
}

impl Deserialize for f64 {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        let bytes = decoder.read_slice(8)?;
        let mut arr = [0u8; 8];
        arr.copy_from_slice(bytes);
        Ok(f64::from_bits(u64::from_le_bytes(arr)))
    }
}

// ---------------------------------------------------------------------------
// String / &str
// ---------------------------------------------------------------------------

impl Serialize for str {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        let bytes = self.as_bytes();
        encoder.write_varint_u64(bytes.len() as u64);
        encoder.push_bytes(bytes);
        Ok(())
    }
}

impl Serialize for String {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        Serialize::serialize(self.as_str(), encoder)
    }
}

impl Deserialize for String {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        let bytes = decoder.read_length_prefixed()?;
        core::str::from_utf8(bytes)
            .map(alloc::string::ToString::to_string)
            .map_err(|_| SerialError::InvalidUtf8)
    }
}

// ---------------------------------------------------------------------------
// Vec<u8> / &[u8]
// ---------------------------------------------------------------------------

impl Serialize for [u8] {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        encoder.write_varint_u64(self.len() as u64);
        encoder.push_bytes(self);
        Ok(())
    }
}

impl Serialize for Vec<u8> {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        Serialize::serialize(self.as_slice(), encoder)
    }
}

impl Deserialize for Vec<u8> {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        let bytes = decoder.read_length_prefixed()?;
        Ok(bytes.to_vec())
    }
}

// ---------------------------------------------------------------------------
// Fixed-size arrays — [T; N]
// ---------------------------------------------------------------------------

impl<T: Serialize, const N: usize> Serialize for [T; N] {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        for item in self {
            item.serialize(encoder)?;
        }
        Ok(())
    }
}

impl<T: Deserialize, const N: usize> Deserialize for [T; N] {
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        // Build the array element-by-element. We can't use [T::deserialize(); N]
        // because T isn't Copy and may not be Default. Use core::array::from_fn
        // with a Result-collecting pattern: write each slot, then on error
        // dropping the partial array will run the elements' destructors.
        //
        // The trick: we use MaybeUninit to avoid requiring T: Default, but
        // since we forbid `unsafe`, we use a `Vec`-then-array path. For small
        // N this is essentially free.
        let mut out: Vec<T> = Vec::with_capacity(N);
        for _ in 0..N {
            out.push(T::deserialize(decoder)?);
        }
        // `Vec<T>::try_into` produces `[T; N]` when the length matches.
        out.try_into().map_err(|_| SerialError::IntegerOutOfRange)
    }
}

// ---------------------------------------------------------------------------
// Tuples — arity 0..=12
// ---------------------------------------------------------------------------

impl Serialize for () {
    #[inline]
    fn serialize(&self, _encoder: &mut Encoder) -> Result<()> {
        Ok(())
    }
}

impl Deserialize for () {
    #[inline]
    fn deserialize(_decoder: &mut Decoder<'_>) -> Result<Self> {
        Ok(())
    }
}

macro_rules! impl_tuple {
    ($($name:ident: $idx:tt),+) => {
        impl<$($name: Serialize),+> Serialize for ($($name,)+) {
            #[inline]
            fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
                $( self.$idx.serialize(encoder)?; )+
                Ok(())
            }
        }

        impl<$($name: Deserialize),+> Deserialize for ($($name,)+) {
            #[inline]
            fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
                Ok(( $( $name::deserialize(decoder)?, )+ ))
            }
        }
    };
}

impl_tuple!(T0: 0);
impl_tuple!(T0: 0, T1: 1);
impl_tuple!(T0: 0, T1: 1, T2: 2);
impl_tuple!(T0: 0, T1: 1, T2: 2, T3: 3);
impl_tuple!(T0: 0, T1: 1, T2: 2, T3: 3, T4: 4);
impl_tuple!(T0: 0, T1: 1, T2: 2, T3: 3, T4: 4, T5: 5);
impl_tuple!(T0: 0, T1: 1, T2: 2, T3: 3, T4: 4, T5: 5, T6: 6);
impl_tuple!(T0: 0, T1: 1, T2: 2, T3: 3, T4: 4, T5: 5, T6: 6, T7: 7);
impl_tuple!(T0: 0, T1: 1, T2: 2, T3: 3, T4: 4, T5: 5, T6: 6, T7: 7, T8: 8);
impl_tuple!(T0: 0, T1: 1, T2: 2, T3: 3, T4: 4, T5: 5, T6: 6, T7: 7, T8: 8, T9: 9);
impl_tuple!(T0: 0, T1: 1, T2: 2, T3: 3, T4: 4, T5: 5, T6: 6, T7: 7, T8: 8, T9: 9, T10: 10);
impl_tuple!(T0: 0, T1: 1, T2: 2, T3: 3, T4: 4, T5: 5, T6: 6, T7: 7, T8: 8, T9: 9, T10: 10, T11: 11);

// ---------------------------------------------------------------------------
// Option<T>
// ---------------------------------------------------------------------------

impl<T: Serialize> Serialize for Option<T> {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            None => {
                encoder.push_byte(0x00);
                Ok(())
            }
            Some(value) => {
                encoder.push_byte(0x01);
                value.serialize(encoder)
            }
        }
    }
}

impl<T: Deserialize> Deserialize for Option<T> {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.read_byte()? {
            0x00 => Ok(None),
            0x01 => Ok(Some(T::deserialize(decoder)?)),
            tag => Err(SerialError::InvalidTag {
                kind: "Option",
                tag,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Result<T, E>
// ---------------------------------------------------------------------------

impl<T: Serialize, E: Serialize> Serialize for core::result::Result<T, E> {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        match self {
            Ok(value) => {
                encoder.push_byte(0x00);
                value.serialize(encoder)
            }
            Err(err) => {
                encoder.push_byte(0x01);
                err.serialize(encoder)
            }
        }
    }
}

impl<T: Deserialize, E: Deserialize> Deserialize for core::result::Result<T, E> {
    #[inline]
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self> {
        match decoder.read_byte()? {
            0x00 => Ok(Ok(T::deserialize(decoder)?)),
            0x01 => Ok(Err(E::deserialize(decoder)?)),
            tag => Err(SerialError::InvalidTag {
                kind: "Result",
                tag,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// References — &T forwards to T's Serialize impl
// ---------------------------------------------------------------------------

impl<T: Serialize + ?Sized> Serialize for &T {
    #[inline]
    fn serialize(&self, encoder: &mut Encoder) -> Result<()> {
        (**self).serialize(encoder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode, encode};
    use alloc::vec;

    fn round_trip<T>(value: T)
    where
        T: Serialize + Deserialize + PartialEq + core::fmt::Debug,
    {
        let bytes = encode(&value).expect("encode");
        let back: T = decode(&bytes).expect("decode");
        assert_eq!(back, value);
    }

    #[test]
    fn u8_round_trips() {
        for v in [0u8, 1, 127, 128, 255] {
            round_trip(v);
        }
    }

    #[test]
    fn u16_round_trips() {
        for v in [0u16, 1, 127, 128, 255, 256, u16::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn u32_round_trips() {
        for v in [0u32, 1, 255, 256, u16::MAX as u32, u32::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn u64_round_trips() {
        for v in [0u64, 1, u32::MAX as u64, u64::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn u128_round_trips() {
        for v in [0u128, 1, u64::MAX as u128, u128::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn usize_round_trips() {
        for v in [0usize, 1, usize::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn i8_round_trips() {
        for v in [0i8, -1, 1, i8::MIN, i8::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn i16_round_trips() {
        for v in [0i16, -1, 1, i16::MIN, i16::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn i32_round_trips() {
        for v in [0i32, -1, 1, i32::MIN, i32::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn i64_round_trips() {
        for v in [0i64, -1, 1, i64::MIN, i64::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn i128_round_trips() {
        for v in [0i128, -1, 1, i128::MIN, i128::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn isize_round_trips() {
        for v in [0isize, -1, 1, isize::MIN, isize::MAX] {
            round_trip(v);
        }
    }

    #[test]
    fn bool_round_trips() {
        round_trip(true);
        round_trip(false);
    }

    #[test]
    fn invalid_bool_byte_is_rejected() {
        let err = decode::<bool>(&[0x7f]).expect_err("0x7f is not a bool");
        assert!(matches!(err, SerialError::InvalidBool { byte: 0x7f }));
    }

    #[test]
    fn f32_round_trips_including_inf() {
        for v in [
            0.0f32,
            -0.0,
            1.0,
            -1.0,
            f32::MIN,
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let bytes = encode(&v).unwrap();
            let back: f32 = decode(&bytes).unwrap();
            assert_eq!(back.to_bits(), v.to_bits());
        }
    }

    #[test]
    fn f64_round_trips_including_inf() {
        for v in [
            0.0f64,
            -0.0,
            1.0,
            -1.0,
            f64::MIN,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            let bytes = encode(&v).unwrap();
            let back: f64 = decode(&bytes).unwrap();
            assert_eq!(back.to_bits(), v.to_bits());
        }
    }

    #[test]
    fn f64_nan_round_trips_bit_for_bit() {
        let v = f64::NAN;
        let bytes = encode(&v).unwrap();
        let back: f64 = decode(&bytes).unwrap();
        assert_eq!(back.to_bits(), v.to_bits());
        assert!(back.is_nan());
    }

    #[test]
    fn string_round_trips() {
        for s in ["", "hello", "a longer string with some content"] {
            round_trip(String::from(s));
        }
    }

    #[test]
    fn string_with_invalid_utf8_is_rejected() {
        // Length 2, then 0xff 0xff (not valid UTF-8 start bytes).
        let bytes = [0x02, 0xff, 0xff];
        let err = decode::<String>(&bytes).expect_err("invalid UTF-8 should fail");
        assert!(matches!(err, SerialError::InvalidUtf8));
    }

    #[test]
    fn str_serializes_like_string() {
        let from_str = encode(&"hello").unwrap();
        let from_string = encode(&String::from("hello")).unwrap();
        assert_eq!(from_str, from_string);
    }

    #[test]
    fn vec_u8_round_trips() {
        round_trip(vec![]);
        round_trip(vec![0u8, 1, 2, 3]);
        round_trip(vec![0xffu8; 1024]);
    }

    #[test]
    fn array_round_trips() {
        round_trip([1u32, 2, 3, 4]);
        round_trip([0u8; 0]); // zero-length array
    }

    #[test]
    fn array_decode_propagates_inner_error() {
        // [u32; 3] needs at least 3 bytes of varint data; give it only one.
        let bytes = [0x80]; // continuation bit set, but no follow-on byte
        let err = decode::<[u32; 3]>(&bytes).expect_err("truncated array should fail");
        assert!(matches!(
            err,
            SerialError::UnexpectedEof { .. } | SerialError::VarintOverflow
        ));
    }

    #[test]
    fn tuple_round_trips() {
        round_trip((1u8, 2u16, 3u32));
        round_trip((true, false, true));
        round_trip((String::from("a"), 42u64, -3i32));
    }

    #[test]
    fn unit_serializes_to_zero_bytes() {
        let bytes = encode(&()).unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn option_round_trips() {
        round_trip::<Option<u64>>(None);
        round_trip::<Option<u64>>(Some(42));
        round_trip::<Option<String>>(Some(String::from("hi")));
    }

    #[test]
    fn option_invalid_tag_is_rejected() {
        let err = decode::<Option<u8>>(&[0x02]).expect_err("0x02 is not a valid Option tag");
        assert!(matches!(
            err,
            SerialError::InvalidTag {
                kind: "Option",
                tag: 0x02
            }
        ));
    }

    #[test]
    fn result_round_trips() {
        round_trip::<core::result::Result<u64, String>>(Ok(7));
        round_trip::<core::result::Result<u64, String>>(Err(String::from("nope")));
    }

    #[test]
    fn result_invalid_tag_is_rejected() {
        let err =
            decode::<core::result::Result<u8, u8>>(&[0x02]).expect_err("0x02 is not a Result tag");
        assert!(matches!(
            err,
            SerialError::InvalidTag {
                kind: "Result",
                tag: 0x02
            }
        ));
    }

    #[test]
    fn nested_round_trip() {
        let value: (Option<String>, Vec<u8>, [u32; 3]) =
            (Some(String::from("nested")), vec![9, 8, 7], [1, 2, 3]);
        round_trip(value);
    }
}
