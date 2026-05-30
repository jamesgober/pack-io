//! `Serialize` / `Deserialize` implementations for primitive, container, and
//! collection types.
//!
//! ## Wire format (full reference: [`docs/WIRE_FORMAT.md`])
//!
//! - `u8` / `i8` — one byte each (fixed). `i8` is two's-complement.
//! - `u16` / `u32` / `u64` / `u128` / `usize` — LEB128 varint. `usize` is
//!   encoded through `u64`; on a 32-bit target a decoded value outside
//!   `usize::MAX` is rejected with [`SerialError::IntegerOutOfRange`].
//! - `i16` / `i32` / `i64` / `i128` / `isize` — ZigZag mapping followed by
//!   LEB128 varint.
//! - `bool` — one byte (`0x00` / `0x01`); any other byte is rejected.
//! - `f32` / `f64` — IEEE 754 bit pattern, little-endian. NaN, ±Inf,
//!   subnormals, and signed zeros all round-trip bit-for-bit.
//! - `String` / `&str` — varint length prefix, then UTF-8 bytes.
//! - `[T; N]` — `N` consecutive `T` encodings, no length prefix (the length
//!   is in the type).
//! - `Vec<T>` / `&[T]` — varint length prefix, then `len` consecutive `T`
//!   encodings.
//! - tuples (arity 1..=12) — fields concatenated in declaration order.
//! - `Option<T>` — one tag byte (`0x00` = `None`, `0x01` = `Some`) followed
//!   by the inner value when present.
//! - `Result<T, E>` — one tag byte (`0x00` = `Ok`, `0x01` = `Err`) followed
//!   by the inner value.
//! - `()` (unit) — zero bytes.
//! - `BTreeMap` / `BTreeSet` / `HashMap` / `HashSet` — varint count followed
//!   by the entries sorted lexicographically by their **encoded key bytes**.
//!   This canonical ordering means a `HashMap` and a `BTreeMap` holding the
//!   same logical data encode to the same bytes, regardless of insertion
//!   order or build-flag-dependent hash randomisation.
//!
//! [`docs/WIRE_FORMAT.md`]: https://github.com/jamesgober/pack-io/blob/main/docs/WIRE_FORMAT.md

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::collections::{HashMap, HashSet};
#[cfg(feature = "std")]
use std::hash::{BuildHasher, Hash};

use crate::codec::{Decode, Encode, Encoder};
use crate::error::{Result, SerialError};
use crate::traits::{Deserialize, Serialize};
use crate::varint;

// ---------------------------------------------------------------------------
// Unsigned integers
// ---------------------------------------------------------------------------

impl Serialize for u8 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_byte(*self)
    }
}

impl Deserialize for u8 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        decoder.read_byte()
    }
}

impl Serialize for u16 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(u64::from(*self))
    }
}

impl Deserialize for u16 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let value = decoder.read_varint_u64()?;
        u16::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

impl Serialize for u32 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(u64::from(*self))
    }
}

impl Deserialize for u32 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let value = decoder.read_varint_u64()?;
        u32::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

impl Serialize for u64 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(*self)
    }
}

impl Deserialize for u64 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        decoder.read_varint_u64()
    }
}

impl Serialize for u128 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u128(*self)
    }
}

impl Deserialize for u128 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        decoder.read_varint_u128()
    }
}

impl Serialize for usize {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(*self as u64)
    }
}

impl Deserialize for usize {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let value = decoder.read_varint_u64()?;
        usize::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

// ---------------------------------------------------------------------------
// Signed integers — ZigZag + varint
// ---------------------------------------------------------------------------

impl Serialize for i8 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_byte(*self as u8)
    }
}

impl Deserialize for i8 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        Ok(decoder.read_byte()? as i8)
    }
}

impl Serialize for i16 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(varint::zigzag_encode_i64(i64::from(*self)))
    }
}

impl Deserialize for i16 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let value = varint::zigzag_decode_i64(decoder.read_varint_u64()?);
        i16::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

impl Serialize for i32 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(varint::zigzag_encode_i64(i64::from(*self)))
    }
}

impl Deserialize for i32 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let value = varint::zigzag_decode_i64(decoder.read_varint_u64()?);
        i32::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

impl Serialize for i64 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(varint::zigzag_encode_i64(*self))
    }
}

impl Deserialize for i64 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        Ok(varint::zigzag_decode_i64(decoder.read_varint_u64()?))
    }
}

impl Serialize for i128 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u128(varint::zigzag_encode_i128(*self))
    }
}

impl Deserialize for i128 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        Ok(varint::zigzag_decode_i128(decoder.read_varint_u128()?))
    }
}

impl Serialize for isize {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(varint::zigzag_encode_i64(*self as i64))
    }
}

impl Deserialize for isize {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let value = varint::zigzag_decode_i64(decoder.read_varint_u64()?);
        isize::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
    }
}

// ---------------------------------------------------------------------------
// Bool
// ---------------------------------------------------------------------------

impl Serialize for bool {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_byte(u8::from(*self))
    }
}

impl Deserialize for bool {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
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
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_bytes(&self.to_bits().to_le_bytes())
    }
}

impl Deserialize for f32 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let mut buf = [0u8; 4];
        decoder.read_into(&mut buf)?;
        Ok(f32::from_bits(u32::from_le_bytes(buf)))
    }
}

impl Serialize for f64 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_bytes(&self.to_bits().to_le_bytes())
    }
}

impl Deserialize for f64 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let mut buf = [0u8; 8];
        decoder.read_into(&mut buf)?;
        Ok(f64::from_bits(u64::from_le_bytes(buf)))
    }
}

// ---------------------------------------------------------------------------
// String / &str
// ---------------------------------------------------------------------------

impl Serialize for str {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        let bytes = self.as_bytes();
        encoder.write_varint_u64(bytes.len() as u64)?;
        encoder.write_bytes(bytes)
    }
}

impl Serialize for String {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        Serialize::serialize(self.as_str(), encoder)
    }
}

impl Deserialize for String {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let bytes = decoder.read_length_prefixed()?;
        String::from_utf8(bytes).map_err(|_| SerialError::InvalidUtf8)
    }
}

// ---------------------------------------------------------------------------
// Slices and Vec<T>
// ---------------------------------------------------------------------------

impl<T: Serialize> Serialize for [T] {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(self.len() as u64)?;
        for item in self {
            item.serialize(encoder)?;
        }
        Ok(())
    }
}

impl<T: Serialize> Serialize for Vec<T> {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        Serialize::serialize(self.as_slice(), encoder)
    }
}

impl<T: Deserialize> Deserialize for Vec<T> {
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count::<T, _>(declared, decoder)?;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push(T::deserialize(decoder)?);
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Fixed-size arrays — [T; N]
// ---------------------------------------------------------------------------

impl<T: Serialize, const N: usize> Serialize for [T; N] {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        for item in self {
            item.serialize(encoder)?;
        }
        Ok(())
    }
}

impl<T: Deserialize, const N: usize> Deserialize for [T; N] {
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let mut out: Vec<T> = Vec::with_capacity(N);
        for _ in 0..N {
            out.push(T::deserialize(decoder)?);
        }
        out.try_into().map_err(|_| SerialError::IntegerOutOfRange)
    }
}

// ---------------------------------------------------------------------------
// Tuples — arity 0..=12
// ---------------------------------------------------------------------------

impl Serialize for () {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, _encoder: &mut E) -> Result<()> {
        Ok(())
    }
}

impl Deserialize for () {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(_decoder: &mut D) -> Result<Self> {
        Ok(())
    }
}

macro_rules! impl_tuple {
    ($($name:ident: $idx:tt),+) => {
        impl<$($name: Serialize),+> Serialize for ($($name,)+) {
            #[inline]
            fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
                $( self.$idx.serialize(encoder)?; )+
                Ok(())
            }
        }

        impl<$($name: Deserialize),+> Deserialize for ($($name,)+) {
            #[inline]
            fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
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
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        match self {
            None => encoder.write_byte(0x00),
            Some(value) => {
                encoder.write_byte(0x01)?;
                value.serialize(encoder)
            }
        }
    }
}

impl<T: Deserialize> Deserialize for Option<T> {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
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
    fn serialize<Enc: Encode + ?Sized>(&self, encoder: &mut Enc) -> Result<()> {
        match self {
            Ok(value) => {
                encoder.write_byte(0x00)?;
                value.serialize(encoder)
            }
            Err(err) => {
                encoder.write_byte(0x01)?;
                err.serialize(encoder)
            }
        }
    }
}

impl<T: Deserialize, E: Deserialize> Deserialize for core::result::Result<T, E> {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
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
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        (**self).serialize(encoder)
    }
}

// ---------------------------------------------------------------------------
// Map and set collections
// ---------------------------------------------------------------------------
//
// Encoding contract: `varint(count) ++ sorted_entries`, where entries are
// sorted lexicographically by their **encoded key bytes**. This means a
// `HashMap` and a `BTreeMap` holding the same logical data encode to the
// same bytes. Hash-randomisation across runs and insertion order are both
// irrelevant to the output — the byte-determinism contract holds.

/// Encode `count` entries as `varint(count) ++ each (key, value) pair`,
/// where entries are pre-sorted by encoded-key bytes.
///
/// `count` is a fresh ascending iteration over the source collection. The
/// helper encodes each `(K, V)` pair to a temporary `Vec<u8>` (capturing the
/// length of the key portion so the sort step does not re-encode), sorts
/// those byte representations, then concatenates them onto `encoder`.
fn encode_map_like<K, V, I, E>(count: usize, entries: I, encoder: &mut E) -> Result<()>
where
    K: Serialize,
    V: Serialize,
    I: IntoIterator<Item = (K, V)>,
    E: Encode + ?Sized,
{
    let mut buffered: Vec<(usize, Vec<u8>)> = Vec::with_capacity(count);
    for (k, v) in entries {
        let mut tmp = Encoder::new();
        k.serialize(&mut tmp)?;
        let key_len = tmp.as_bytes().len();
        v.serialize(&mut tmp)?;
        buffered.push((key_len, tmp.into_inner()));
    }
    buffered.sort_by(|a, b| {
        let ka = &a.1[..a.0];
        let kb = &b.1[..b.0];
        ka.cmp(kb)
    });
    encoder.write_varint_u64(count as u64)?;
    for (_, bytes) in &buffered {
        encoder.write_bytes(bytes)?;
    }
    Ok(())
}

/// Encode `count` set elements as `varint(count) ++ sorted_elements`.
fn encode_set_like<T, I, E>(count: usize, items: I, encoder: &mut E) -> Result<()>
where
    T: Serialize,
    I: IntoIterator<Item = T>,
    E: Encode + ?Sized,
{
    let mut buffered: Vec<Vec<u8>> = Vec::with_capacity(count);
    for item in items {
        let mut tmp = Encoder::new();
        item.serialize(&mut tmp)?;
        buffered.push(tmp.into_inner());
    }
    buffered.sort();
    encoder.write_varint_u64(count as u64)?;
    for bytes in &buffered {
        encoder.write_bytes(bytes)?;
    }
    Ok(())
}

impl<K, V> Serialize for BTreeMap<K, V>
where
    K: Serialize,
    V: Serialize,
{
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encode_map_like(self.len(), self.iter(), encoder)
    }
}

impl<K, V> Deserialize for BTreeMap<K, V>
where
    K: Deserialize + Ord,
    V: Deserialize,
{
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count::<(K, V), _>(declared, decoder)?;
        let mut out = BTreeMap::new();
        for _ in 0..len {
            let k = K::deserialize(decoder)?;
            let v = V::deserialize(decoder)?;
            let _ = out.insert(k, v);
        }
        Ok(out)
    }
}

impl<T> Serialize for BTreeSet<T>
where
    T: Serialize,
{
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encode_set_like(self.len(), self.iter(), encoder)
    }
}

impl<T> Deserialize for BTreeSet<T>
where
    T: Deserialize + Ord,
{
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count::<T, _>(declared, decoder)?;
        let mut out = BTreeSet::new();
        for _ in 0..len {
            let _ = out.insert(T::deserialize(decoder)?);
        }
        Ok(out)
    }
}

#[cfg(feature = "std")]
impl<K, V, S> Serialize for HashMap<K, V, S>
where
    K: Serialize,
    V: Serialize,
{
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encode_map_like(self.len(), self.iter(), encoder)
    }
}

#[cfg(feature = "std")]
impl<K, V, S> Deserialize for HashMap<K, V, S>
where
    K: Deserialize + Hash + Eq,
    V: Deserialize,
    S: BuildHasher + Default,
{
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count::<(K, V), _>(declared, decoder)?;
        let mut out = HashMap::with_capacity_and_hasher(len, S::default());
        for _ in 0..len {
            let k = K::deserialize(decoder)?;
            let v = V::deserialize(decoder)?;
            let _ = out.insert(k, v);
        }
        Ok(out)
    }
}

#[cfg(feature = "std")]
impl<T, S> Serialize for HashSet<T, S>
where
    T: Serialize,
{
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encode_set_like(self.len(), self.iter(), encoder)
    }
}

#[cfg(feature = "std")]
impl<T, S> Deserialize for HashSet<T, S>
where
    T: Deserialize + Hash + Eq,
    S: BuildHasher + Default,
{
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count::<T, _>(declared, decoder)?;
        let mut out = HashSet::with_capacity_and_hasher(len, S::default());
        for _ in 0..len {
            let _ = out.insert(T::deserialize(decoder)?);
        }
        Ok(out)
    }
}

/// Validate `declared` (an element count) against the decoder's
/// `max_alloc`, treating each element as occupying at least one byte. This
/// prevents the obvious "declare `u64::MAX` elements, force a giant
/// `Vec::with_capacity`" attack — declaring more elements than the decoder
/// could ever supply bytes for is refused before we allocate.
#[inline]
fn guard_element_count<T, D: Decode + ?Sized>(declared: u64, decoder: &D) -> Result<usize> {
    let max = decoder.max_alloc() as u64;
    if declared > max {
        return Err(SerialError::InvalidLength {
            declared,
            remaining: 0,
        });
    }
    // Type tag silences the unused-type-parameter lint and documents intent.
    let _phantom: core::marker::PhantomData<T> = core::marker::PhantomData;
    usize::try_from(declared).map_err(|_| SerialError::IntegerOutOfRange)
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
    fn u64_round_trips() {
        for v in [0u64, 1, u32::MAX as u64, u64::MAX] {
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
    fn bool_round_trips() {
        round_trip(true);
        round_trip(false);
    }

    #[test]
    fn string_round_trips() {
        for s in ["", "hello", "a longer string with some content"] {
            round_trip(String::from(s));
        }
    }

    #[test]
    fn vec_u8_round_trips() {
        round_trip::<Vec<u8>>(vec![]);
        round_trip::<Vec<u8>>(vec![0u8, 1, 2, 3]);
        round_trip::<Vec<u8>>(vec![0xffu8; 1024]);
    }

    #[test]
    fn vec_u32_round_trips() {
        round_trip::<Vec<u32>>(vec![]);
        round_trip::<Vec<u32>>(vec![1, 2, u32::MAX]);
    }

    #[test]
    fn vec_string_round_trips() {
        round_trip(vec![
            String::from("hello"),
            String::from("world"),
            String::new(),
        ]);
    }

    #[test]
    fn array_round_trips() {
        round_trip([1u32, 2, 3, 4]);
        round_trip([0u8; 0]);
    }

    #[test]
    fn tuple_round_trips() {
        round_trip((1u8, 2u16, 3u32));
        round_trip((String::from("a"), 42u64, -3i32));
    }

    #[test]
    fn option_round_trips() {
        round_trip::<Option<u64>>(None);
        round_trip::<Option<u64>>(Some(42));
        round_trip::<Option<String>>(Some(String::from("hi")));
    }

    #[test]
    fn result_round_trips() {
        round_trip::<core::result::Result<u64, String>>(Ok(7));
        round_trip::<core::result::Result<u64, String>>(Err(String::from("nope")));
    }

    #[test]
    fn invalid_bool_byte_is_rejected() {
        let err = decode::<bool>(&[0x7f]).expect_err("0x7f is not a bool");
        assert!(matches!(err, SerialError::InvalidBool { byte: 0x7f }));
    }

    #[test]
    fn string_with_invalid_utf8_is_rejected() {
        let bytes = [0x02, 0xff, 0xff];
        let err = decode::<String>(&bytes).expect_err("invalid UTF-8 should fail");
        assert!(matches!(err, SerialError::InvalidUtf8));
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
    fn btreemap_round_trips() {
        let mut m = BTreeMap::new();
        let _ = m.insert(String::from("a"), 1u32);
        let _ = m.insert(String::from("b"), 2);
        let _ = m.insert(String::from("c"), 3);
        round_trip(m);
    }

    #[test]
    fn btreemap_empty_round_trips() {
        round_trip(BTreeMap::<u32, u32>::new());
    }

    #[test]
    fn btreeset_round_trips() {
        let mut s = BTreeSet::new();
        let _ = s.insert(1u32);
        let _ = s.insert(2);
        let _ = s.insert(3);
        round_trip(s);
    }

    #[cfg(feature = "std")]
    #[test]
    fn hashmap_round_trips() {
        let mut m: HashMap<String, u32> = HashMap::new();
        let _ = m.insert(String::from("alpha"), 1);
        let _ = m.insert(String::from("beta"), 2);
        round_trip(m);
    }

    #[cfg(feature = "std")]
    #[test]
    fn hashset_round_trips() {
        let mut s: HashSet<u32> = HashSet::new();
        let _ = s.insert(1);
        let _ = s.insert(7);
        let _ = s.insert(42);
        round_trip(s);
    }

    #[cfg(feature = "std")]
    #[test]
    fn hashmap_and_btreemap_encode_identically_for_same_data() {
        let mut h: HashMap<String, u32> = HashMap::new();
        let mut b: BTreeMap<String, u32> = BTreeMap::new();
        for (k, v) in [("zeta", 5u32), ("alpha", 1), ("delta", 4), ("beta", 2)] {
            let _ = h.insert(k.into(), v);
            let _ = b.insert(k.into(), v);
        }
        assert_eq!(encode(&h).unwrap(), encode(&b).unwrap());
    }

    #[cfg(feature = "std")]
    #[test]
    fn hashmap_insertion_order_independent() {
        let mut a: HashMap<u32, u32> = HashMap::new();
        let _ = a.insert(1, 10);
        let _ = a.insert(2, 20);
        let _ = a.insert(3, 30);

        let mut b: HashMap<u32, u32> = HashMap::new();
        let _ = b.insert(3, 30);
        let _ = b.insert(1, 10);
        let _ = b.insert(2, 20);

        assert_eq!(encode(&a).unwrap(), encode(&b).unwrap());
    }

    #[test]
    fn collection_with_hostile_element_count_is_rejected() {
        // varint(u64::MAX) is 10 bytes of 0xff... 0x01.
        let bytes: [u8; 10] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
        let err = decode::<Vec<u32>>(&bytes).expect_err("hostile count");
        assert!(matches!(err, SerialError::InvalidLength { .. }));
    }
}
