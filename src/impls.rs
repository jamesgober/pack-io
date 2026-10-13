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
//! - `char` — the Unicode scalar value as a `u32` varint; surrogates and
//!   values above `0x10FFFF` are rejected.
//! - `String` / `&str` — varint length prefix, then UTF-8 bytes.
//! - `[T; N]` — `N` consecutive `T` encodings, no length prefix (the length
//!   is in the type).
//! - `Vec<T>` / `&[T]` / `VecDeque<T>` — varint length prefix, then `len`
//!   consecutive `T` encodings.
//! - tuples (arity 1..=12) — fields concatenated in declaration order.
//! - `Option<T>` — one tag byte (`0x00` = `None`, `0x01` = `Some`) followed
//!   by the inner value when present.
//! - `Result<T, E>` — one tag byte (`0x00` = `Ok`, `0x01` = `Err`) followed
//!   by the inner value.
//! - `()` and `PhantomData<T>` — zero bytes.
//! - `NonZero*` — the same bytes as the underlying integer; zero is rejected.
//! - `Duration` — `varint(secs: u64) ++ varint(nanos: u32)`, nanos `< 10^9`.
//! - `Box<T>` (including `Box<str>`, `Box<[T]>`), `Rc<T>`, `Arc<T>`,
//!   `Cow<'_, T>` — transparent:
//!   the same bytes as the value they hold.
//! - `BTreeMap` / `BTreeSet` / `HashMap` / `HashSet` — varint count followed
//!   by the entries sorted lexicographically by their **encoded key bytes**.
//!   This canonical ordering means a `HashMap` and a `BTreeMap` holding the
//!   same logical data encode to the same bytes, regardless of insertion
//!   order or build-flag-dependent hash randomisation.
//!
//! [`docs/WIRE_FORMAT.md`]: https://github.com/jamesgober/pack-io/blob/main/docs/WIRE_FORMAT.md

use alloc::borrow::{Cow, ToOwned};
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::rc::Rc;
use alloc::string::String;
#[cfg(target_has_atomic = "ptr")]
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::num::{
    NonZeroI8, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI128, NonZeroIsize, NonZeroU8,
    NonZeroU16, NonZeroU32, NonZeroU64, NonZeroU128, NonZeroUsize,
};
use core::time::Duration;
#[cfg(feature = "std")]
use std::collections::{HashMap, HashSet};
#[cfg(feature = "std")]
use std::hash::{BuildHasher, Hash};

#[cfg(feature = "std")]
use crate::codec::initial_capacity;
use crate::codec::{Decode, Encode, Encoder, guard_element_count, varint_len};
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

    /// Single bulk write — bypasses the per-element loop the default impl
    /// would emit. For an in-memory [`crate::Encoder`] this compiles to a
    /// single `Vec::extend_from_slice`; for [`crate::IoEncoder`] it is one
    /// `Write::write_all`. Either way it replaces N per-byte calls with a
    /// memcpy.
    #[inline]
    fn serialize_slice<E: Encode + ?Sized>(slice: &[u8], encoder: &mut E) -> Result<()> {
        encoder.write_bytes(slice)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(1)
    }

    #[inline]
    fn encoded_len_slice(slice: &[u8]) -> Result<usize> {
        Ok(slice.len())
    }
}

impl Deserialize for u8 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        decoder.read_byte()
    }

    /// Single bulk read. For an in-memory [`crate::Decoder`] this is a
    /// bounds check plus one copy of exactly `count` bytes; a `count` the
    /// input cannot back is refused before anything is allocated. For
    /// [`crate::IoDecoder`] the buffer grows in bounded chunks as bytes
    /// arrive, so a hostile count over a short stream costs one chunk.
    #[inline]
    fn deserialize_many<D: Decode + ?Sized>(decoder: &mut D, count: usize) -> Result<Vec<u8>> {
        decoder.read_byte_cow(count).map(Cow::into_owned)
    }

    /// `[u8; N]` decodes with one read into a stack array — no heap.
    #[inline]
    fn deserialize_array<D: Decode + ?Sized, const N: usize>(decoder: &mut D) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        decoder.read_into(&mut out)?;
        Ok(out)
    }
}

impl Serialize for u16 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(u64::from(*self))
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(u64::from(*self)))
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(u64::from(*self)))
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(*self))
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint::len_u128(*self))
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(*self as u64))
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(1)
    }
}

impl Deserialize for i8 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        Ok(decoder.read_byte()? as i8)
    }
}

/// `Serialize` / `Deserialize` for a signed integer narrower than 64 bits
/// (or pointer-sized): ZigZag through `i64`, then a `u64` varint.
macro_rules! impl_signed_varint {
    ($($t:ty),+ $(,)?) => {
        $(
            impl Serialize for $t {
                #[inline]
                fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
                    encoder.write_varint_u64(varint::zigzag_encode_i64(*self as i64))
                }

                #[inline]
                fn encoded_len(&self) -> Result<usize> {
                    Ok(varint_len(varint::zigzag_encode_i64(*self as i64)))
                }
            }

            impl Deserialize for $t {
                #[inline]
                fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
                    let value = varint::zigzag_decode_i64(decoder.read_varint_u64()?);
                    <$t>::try_from(value).map_err(|_| SerialError::IntegerOutOfRange)
                }
            }
        )+
    };
}

impl_signed_varint!(i16, i32, isize);

impl Serialize for i64 {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(varint::zigzag_encode_i64(*self))
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(varint::zigzag_encode_i64(*self)))
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint::len_u128(varint::zigzag_encode_i128(*self)))
    }
}

impl Deserialize for i128 {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        Ok(varint::zigzag_decode_i128(decoder.read_varint_u128()?))
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(1)
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
// char — Unicode scalar value as a u32 varint
// ---------------------------------------------------------------------------

impl Serialize for char {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(u64::from(u32::from(*self)))
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(u64::from(u32::from(*self))))
    }
}

impl Deserialize for char {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let scalar = u32::deserialize(decoder)?;
        char::from_u32(scalar).ok_or(SerialError::InvalidValue { kind: "char" })
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(4)
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(8)
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
// String / &str / Box<str>
// ---------------------------------------------------------------------------

impl Serialize for str {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        let bytes = self.as_bytes();
        encoder.write_varint_u64(bytes.len() as u64)?;
        encoder.write_bytes(bytes)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(self.len() as u64).saturating_add(self.len()))
    }
}

impl Serialize for String {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        Serialize::serialize(self.as_str(), encoder)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        self.as_str().encoded_len()
    }
}

impl Deserialize for String {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let bytes = decoder.read_length_prefixed()?;
        String::from_utf8(bytes).map_err(|_| SerialError::InvalidUtf8)
    }
}

impl Deserialize for Box<str> {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        String::deserialize(decoder).map(String::into_boxed_str)
    }
}

// ---------------------------------------------------------------------------
// Slices, Vec<T>, Box<[T]>, VecDeque<T>
// ---------------------------------------------------------------------------

impl<T: Serialize> Serialize for [T] {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(self.len() as u64)?;
        // Dispatch to T's `serialize_slice` so byte-slice payloads
        // (T = u8) take the memcpy fast path via the `u8` override.
        T::serialize_slice(self, encoder)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(self.len() as u64).saturating_add(T::encoded_len_slice(self)?))
    }
}

impl<T: Serialize> Serialize for Vec<T> {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        Serialize::serialize(self.as_slice(), encoder)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        self.as_slice().encoded_len()
    }
}

impl<T: Deserialize> Deserialize for Vec<T> {
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count(declared, decoder)?;
        // Dispatch to T's `deserialize_many` so byte-vec payloads
        // (T = u8) take the memcpy fast path via the `u8` override.
        T::deserialize_many(decoder, len)
    }
}

impl<T: Deserialize> Deserialize for Box<[T]> {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        Vec::<T>::deserialize(decoder).map(Vec::into_boxed_slice)
    }
}

impl<T: Serialize> Serialize for VecDeque<T> {
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(self.len() as u64)?;
        let (front, back) = self.as_slices();
        T::serialize_slice(front, encoder)?;
        T::serialize_slice(back, encoder)
    }

    fn encoded_len(&self) -> Result<usize> {
        let (front, back) = self.as_slices();
        Ok(varint_len(self.len() as u64)
            .saturating_add(T::encoded_len_slice(front)?)
            .saturating_add(T::encoded_len_slice(back)?))
    }
}

impl<T: Deserialize> Deserialize for VecDeque<T> {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        Vec::<T>::deserialize(decoder).map(VecDeque::from)
    }
}

// ---------------------------------------------------------------------------
// Fixed-size arrays — [T; N]
// ---------------------------------------------------------------------------

impl<T: Serialize, const N: usize> Serialize for [T; N] {
    /// Routed through `T::serialize_slice`, so `[u8; N]` is one bulk write.
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        T::serialize_slice(self, encoder)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        T::encoded_len_slice(self)
    }
}

impl<T: Deserialize, const N: usize> Deserialize for [T; N] {
    /// Routed through `T::deserialize_array`, so `[u8; N]` is one read into
    /// a stack array.
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        T::deserialize_array::<D, N>(decoder)
    }
}

// ---------------------------------------------------------------------------
// Tuples — arity 0..=12, PhantomData
// ---------------------------------------------------------------------------

impl Serialize for () {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, _encoder: &mut E) -> Result<()> {
        Ok(())
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(0)
    }
}

impl Deserialize for () {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(_decoder: &mut D) -> Result<Self> {
        Ok(())
    }
}

impl<T: ?Sized> Serialize for PhantomData<T> {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, _encoder: &mut E) -> Result<()> {
        Ok(())
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(0)
    }
}

impl<T: ?Sized> Deserialize for PhantomData<T> {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(_decoder: &mut D) -> Result<Self> {
        Ok(PhantomData)
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

            #[inline]
            fn encoded_len(&self) -> Result<usize> {
                let mut total = 0usize;
                $( total = total.saturating_add(self.$idx.encoded_len()?); )+
                Ok(total)
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        match self {
            None => Ok(1),
            Some(value) => Ok(value.encoded_len()?.saturating_add(1)),
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

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        let inner = match self {
            Ok(value) => value.encoded_len()?,
            Err(err) => err.encoded_len()?,
        };
        Ok(inner.saturating_add(1))
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
// Transparent wrappers: &T, Rc<T>, Arc<T>, Cow<'_, T>
// ---------------------------------------------------------------------------

impl<T: Serialize + ?Sized> Serialize for &T {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        (**self).serialize(encoder)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        (**self).encoded_len()
    }
}

impl<T: Serialize + ?Sized> Serialize for Box<T> {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        (**self).serialize(encoder)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        (**self).encoded_len()
    }
}

/// `Box<T>` encodes exactly like `T`, so a recursive type such as
/// `struct List { next: Option<Box<List>> }` derives directly. Recursion
/// through it is bounded by [`crate::Config::max_depth`] because the
/// recursive type itself is derived.
impl<T: Deserialize> Deserialize for Box<T> {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        T::deserialize(decoder).map(Box::new)
    }
}

/// Transparent `Serialize` plus owned `Deserialize` for a shared-ownership
/// pointer. `Rc<T>` / `Arc<T>` encode exactly like `T`; decoding allocates
/// a fresh pointer (sharing is not preserved across the wire).
macro_rules! impl_shared_pointer {
    ($ptr:ident) => {
        impl<T: Serialize + ?Sized> Serialize for $ptr<T> {
            #[inline]
            fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
                (**self).serialize(encoder)
            }

            #[inline]
            fn encoded_len(&self) -> Result<usize> {
                (**self).encoded_len()
            }
        }

        impl<T: Deserialize> Deserialize for $ptr<T> {
            #[inline]
            fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
                T::deserialize(decoder).map($ptr::new)
            }
        }

        impl Deserialize for $ptr<str> {
            #[inline]
            fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
                String::deserialize(decoder).map($ptr::from)
            }
        }

        impl<T: Deserialize> Deserialize for $ptr<[T]> {
            #[inline]
            fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
                Vec::<T>::deserialize(decoder).map($ptr::from)
            }
        }
    };
}

impl_shared_pointer!(Rc);
#[cfg(target_has_atomic = "ptr")]
impl_shared_pointer!(Arc);

impl<B> Serialize for Cow<'_, B>
where
    B: Serialize + ToOwned + ?Sized,
{
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        (**self).serialize(encoder)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        (**self).encoded_len()
    }
}

/// Owning decode always yields [`Cow::Owned`]. To borrow from the input
/// buffer, decode `Cow<'a, str>` / `Cow<'a, [u8]>` through
/// [`crate::DeserializeView`] instead, which yields [`Cow::Borrowed`].
impl<B> Deserialize for Cow<'_, B>
where
    B: ToOwned + ?Sized,
    B::Owned: Deserialize,
{
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        B::Owned::deserialize(decoder).map(Cow::Owned)
    }
}

// ---------------------------------------------------------------------------
// NonZero integers — same bytes as the underlying integer, zero rejected
// ---------------------------------------------------------------------------

macro_rules! impl_non_zero {
    ($($nz:ident => $prim:ty),+ $(,)?) => {
        $(
            impl Serialize for $nz {
                #[inline]
                fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
                    self.get().serialize(encoder)
                }

                #[inline]
                fn encoded_len(&self) -> Result<usize> {
                    self.get().encoded_len()
                }
            }

            impl Deserialize for $nz {
                #[inline]
                fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
                    let raw = <$prim>::deserialize(decoder)?;
                    $nz::new(raw).ok_or(SerialError::InvalidValue { kind: stringify!($nz) })
                }
            }
        )+
    };
}

impl_non_zero!(
    NonZeroU8 => u8,
    NonZeroU16 => u16,
    NonZeroU32 => u32,
    NonZeroU64 => u64,
    NonZeroU128 => u128,
    NonZeroUsize => usize,
    NonZeroI8 => i8,
    NonZeroI16 => i16,
    NonZeroI32 => i32,
    NonZeroI64 => i64,
    NonZeroI128 => i128,
    NonZeroIsize => isize,
);

// ---------------------------------------------------------------------------
// Duration — varint(secs) ++ varint(subsec nanos)
// ---------------------------------------------------------------------------

impl Serialize for Duration {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(self.as_secs())?;
        encoder.write_varint_u64(u64::from(self.subsec_nanos()))
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(self.as_secs()).saturating_add(varint_len(u64::from(self.subsec_nanos()))))
    }
}

impl Deserialize for Duration {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        const NANOS_PER_SEC: u32 = 1_000_000_000;
        let secs = decoder.read_varint_u64()?;
        let nanos = u32::deserialize(decoder)?;
        if nanos >= NANOS_PER_SEC {
            return Err(SerialError::InvalidValue { kind: "Duration" });
        }
        Ok(Duration::new(secs, nanos))
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

/// Byte offsets of one encoded entry inside the shared scratch buffer:
/// the key occupies `start..key_end`, the whole entry `start..end`.
#[derive(Clone, Copy)]
struct EntrySpan {
    start: usize,
    key_end: usize,
    end: usize,
}

/// Collects the encoded entries of a map or set so they can be written in
/// canonical (encoded-key-byte) order.
///
/// Every entry is encoded once into a single shared scratch buffer while
/// its byte span is recorded, so a whole map costs two growing allocations
/// (the scratch bytes and the span list) instead of one per entry. When the
/// entries already arrive in canonical order — the common case for a
/// `BTreeMap` whose `Ord` agrees with the encoded-byte order — the scratch
/// buffer is emitted in one write and the sort is skipped.
///
/// Ties on the key bytes (only possible for key types whose encoding is not
/// injective) are broken by the full entry bytes, so the output never
/// depends on iteration order.
///
/// Shared by the native map / set impls and the `serde` adapter, which is
/// how a serde map encodes to the same bytes as the native one.
pub(crate) struct SortedEntries {
    scratch: Encoder,
    spans: Vec<EntrySpan>,
    in_order: bool,
    start: usize,
    key_end: Option<usize>,
}

impl SortedEntries {
    /// Room for `count` entries (the span list is bounded by the caller's
    /// already-materialised collection, never by untrusted input).
    pub(crate) fn with_capacity(count: usize) -> Self {
        Self {
            scratch: Encoder::new(),
            spans: Vec::with_capacity(count),
            in_order: true,
            start: 0,
            key_end: None,
        }
    }

    /// The scratch encoder the next key / value must be written into.
    pub(crate) fn encoder(&mut self) -> &mut Encoder {
        &mut self.scratch
    }

    /// Mark the start of a new entry.
    pub(crate) fn begin_entry(&mut self) {
        self.start = self.scratch.as_bytes().len();
        self.key_end = None;
    }

    /// Mark the end of the current entry's key. Sets never call this: their
    /// whole entry is the key.
    pub(crate) fn end_key(&mut self) {
        self.key_end = Some(self.scratch.as_bytes().len());
    }

    /// Finish the current entry.
    pub(crate) fn end_entry(&mut self) {
        let end = self.scratch.as_bytes().len();
        let span = EntrySpan {
            start: self.start,
            key_end: self.key_end.unwrap_or(end),
            end,
        };
        if let Some(prev) = self.spans.last() {
            let bytes = self.scratch.as_bytes();
            self.in_order &= bytes[prev.start..prev.key_end] < bytes[span.start..span.key_end];
        }
        self.spans.push(span);
    }

    /// Write `varint(count) ++ entries in canonical order` into `encoder`.
    pub(crate) fn write_to<E: Encode + ?Sized>(mut self, encoder: &mut E) -> Result<()> {
        encoder.write_varint_u64(self.spans.len() as u64)?;
        let bytes = self.scratch.as_bytes();
        if self.in_order {
            return encoder.write_bytes(bytes);
        }
        self.spans.sort_unstable_by(|a, b| {
            bytes[a.start..a.key_end]
                .cmp(&bytes[b.start..b.key_end])
                .then_with(|| bytes[a.start..a.end].cmp(&bytes[b.start..b.end]))
        });
        for span in &self.spans {
            encoder.write_bytes(&bytes[span.start..span.end])?;
        }
        Ok(())
    }
}

/// Encode `count` entries as `varint(count) ++ entries sorted by encoded
/// key bytes` (see [`SortedEntries`]).
fn encode_map_like<K, V, I, E>(count: usize, entries: I, encoder: &mut E) -> Result<()>
where
    K: Serialize,
    V: Serialize,
    I: IntoIterator<Item = (K, V)>,
    E: Encode + ?Sized,
{
    if count == 0 {
        return encoder.write_byte(0);
    }
    let mut sorted = SortedEntries::with_capacity(count);
    for (k, v) in entries {
        sorted.begin_entry();
        k.serialize(sorted.encoder())?;
        sorted.end_key();
        v.serialize(sorted.encoder())?;
        sorted.end_entry();
    }
    sorted.write_to(encoder)
}

/// Encode `count` set elements as `varint(count) ++ sorted_elements`.
fn encode_set_like<T, I, E>(count: usize, items: I, encoder: &mut E) -> Result<()>
where
    T: Serialize,
    I: IntoIterator<Item = T>,
    E: Encode + ?Sized,
{
    if count == 0 {
        return encoder.write_byte(0);
    }
    let mut sorted = SortedEntries::with_capacity(count);
    for item in items {
        sorted.begin_entry();
        item.serialize(sorted.encoder())?;
        sorted.end_entry();
    }
    sorted.write_to(encoder)
}

/// `encoded_len` of a map: the count prefix plus every entry. Order does
/// not change the length, so no sorting (and no allocation) is needed.
fn map_encoded_len<'a, K, V, I>(count: usize, entries: I) -> Result<usize>
where
    K: Serialize + 'a,
    V: Serialize + 'a,
    I: IntoIterator<Item = (&'a K, &'a V)>,
{
    entries
        .into_iter()
        .try_fold(varint_len(count as u64), |acc, (k, v)| {
            Ok(acc
                .saturating_add(k.encoded_len()?)
                .saturating_add(v.encoded_len()?))
        })
}

/// `encoded_len` of a set: the count prefix plus every element.
fn set_encoded_len<'a, T, I>(count: usize, items: I) -> Result<usize>
where
    T: Serialize + 'a,
    I: IntoIterator<Item = &'a T>,
{
    items
        .into_iter()
        .try_fold(varint_len(count as u64), |acc, item| {
            Ok(acc.saturating_add(item.encoded_len()?))
        })
}

/// Tracks the byte span of the previous key while a map or set decodes in
/// canonical mode, and rejects any key that does not sort strictly after
/// it.
///
/// The spans are offsets into [`Decode::consumed`], which only grows during
/// a decode, so the previous key's bytes stay addressable without copying.
pub(crate) struct KeyOrder {
    reason: &'static str,
    prev: Option<(usize, usize)>,
}

impl KeyOrder {
    /// Start tracking; `reason` is reported in
    /// [`SerialError::NonCanonical`] (`"map key order"` / `"set element
    /// order"`).
    pub(crate) fn new(reason: &'static str) -> Self {
        Self { reason, prev: None }
    }

    /// Offset at which the next key starts.
    ///
    /// # Errors
    ///
    /// [`SerialError::InvalidConfig`] if the decoder cannot expose its
    /// consumed input, so canonical order cannot be checked.
    pub(crate) fn mark<D: Decode + ?Sized>(&self, decoder: &D) -> Result<usize> {
        decoder
            .consumed()
            .map(<[u8]>::len)
            .ok_or(SerialError::InvalidConfig {
                reason: "canonical mode needs a decoder that exposes its consumed input",
            })
    }

    /// Check the key that started at `start` and has just been decoded.
    ///
    /// # Errors
    ///
    /// [`SerialError::NonCanonical`] if the key is not strictly greater
    /// than the previous one.
    pub(crate) fn check<D: Decode + ?Sized>(&mut self, decoder: &D, start: usize) -> Result<()> {
        let consumed = decoder.consumed().unwrap_or_default();
        let end = consumed.len();
        if let Some((prev_start, prev_end)) = self.prev {
            let prev = consumed.get(prev_start..prev_end).unwrap_or_default();
            let current = consumed.get(start..end).unwrap_or_default();
            if current <= prev {
                return Err(SerialError::NonCanonical {
                    reason: self.reason,
                });
            }
        }
        self.prev = Some((start, end));
        Ok(())
    }
}

/// Decode `len` `(K, V)` entries, handing each to `insert`. In canonical
/// mode the encoded keys must be strictly increasing.
fn decode_entries<K, V, D, F>(decoder: &mut D, len: usize, mut insert: F) -> Result<()>
where
    K: Deserialize,
    V: Deserialize,
    D: Decode + ?Sized,
    F: FnMut(K, V),
{
    if decoder.config().canonical {
        let mut order = KeyOrder::new("map key order");
        for _ in 0..len {
            let start = order.mark(decoder)?;
            let k = K::deserialize(decoder)?;
            order.check(decoder, start)?;
            let v = V::deserialize(decoder)?;
            insert(k, v);
        }
    } else {
        for _ in 0..len {
            let k = K::deserialize(decoder)?;
            let v = V::deserialize(decoder)?;
            insert(k, v);
        }
    }
    Ok(())
}

/// Decode `len` set elements, handing each to `insert`. In canonical mode
/// the encoded elements must be strictly increasing.
fn decode_elements<T, D, F>(decoder: &mut D, len: usize, mut insert: F) -> Result<()>
where
    T: Deserialize,
    D: Decode + ?Sized,
    F: FnMut(T),
{
    if decoder.config().canonical {
        let mut order = KeyOrder::new("set element order");
        for _ in 0..len {
            let start = order.mark(decoder)?;
            let item = T::deserialize(decoder)?;
            order.check(decoder, start)?;
            insert(item);
        }
    } else {
        for _ in 0..len {
            insert(T::deserialize(decoder)?);
        }
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

    fn encoded_len(&self) -> Result<usize> {
        map_encoded_len(self.len(), self.iter())
    }
}

impl<K, V> Deserialize for BTreeMap<K, V>
where
    K: Deserialize + Ord,
    V: Deserialize,
{
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count(declared, decoder)?;
        let mut out = BTreeMap::new();
        decode_entries(decoder, len, |k, v| {
            let _ = out.insert(k, v);
        })?;
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

    fn encoded_len(&self) -> Result<usize> {
        set_encoded_len(self.len(), self.iter())
    }
}

impl<T> Deserialize for BTreeSet<T>
where
    T: Deserialize + Ord,
{
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count(declared, decoder)?;
        let mut out = BTreeSet::new();
        decode_elements(decoder, len, |item| {
            let _ = out.insert(item);
        })?;
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

    fn encoded_len(&self) -> Result<usize> {
        map_encoded_len(self.len(), self.iter())
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
        let len = guard_element_count(declared, decoder)?;
        let capacity = initial_capacity::<(K, V), D>(len, decoder);
        let mut out = HashMap::with_capacity_and_hasher(capacity, S::default());
        decode_entries(decoder, len, |k, v| {
            let _ = out.insert(k, v);
        })?;
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

    fn encoded_len(&self) -> Result<usize> {
        set_encoded_len(self.len(), self.iter())
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
        let len = guard_element_count(declared, decoder)?;
        let capacity = initial_capacity::<T, D>(len, decoder);
        let mut out = HashSet::with_capacity_and_hasher(capacity, S::default());
        decode_elements(decoder, len, |item| {
            let _ = out.insert(item);
        })?;
        Ok(out)
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
