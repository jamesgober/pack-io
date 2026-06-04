//! Zero-copy decoding: the [`DeserializeView`] trait, the [`decode_view`]
//! free function, and built-in impls for the types that can borrow directly
//! from the input buffer.
//!
//! ## The two decode surfaces
//!
//! [`Deserialize`] produces an **owning** value — `String`s and `Vec<u8>`s
//! are allocated, the input buffer can be dropped immediately after the
//! call. Use it when the input is transient (a network buffer being
//! recycled, a file being streamed) or when downstream code holds the
//! decoded value for an unbounded lifetime.
//!
//! [`DeserializeView`] produces a **borrowed** value — `&'a str` and
//! `&'a [u8]` fields point directly into the input slice. No per-field
//! allocation. Use it when the input buffer outlives the decoded value
//! (e.g. a memory-mapped file, an arena, a request-lifetime byte buffer).
//!
//! Both surfaces share the same on-wire format. A value encoded with
//! [`crate::encode`] can be read with either [`crate::decode`] (owning) or
//! [`decode_view`] (borrowed); choose the one that matches the lifetime
//! relationship the caller has with the source bytes.
//!
//! ## Built-in implementors
//!
//! - `&'a str`, `&'a [u8]` — the headline zero-copy types.
//! - Every primitive (`u8` … `u128`, `i8` … `i128`, `usize`, `isize`,
//!   `bool`, `f32`, `f64`, `()`) — `DeserializeView` reduces to
//!   [`Deserialize`] for fixed-size scalars, no borrow involved.
//! - `Option<T>`, `Result<T, E>`, tuples `(T1, …, Tn)` (arity 1–12),
//!   fixed-size arrays `[T; N]`.
//! - `Vec<T>` and the standard `BTreeMap` / `BTreeSet` / `HashMap` /
//!   `HashSet` — these have to allocate the container itself, but their
//!   element / key / value types may still be borrows.
//!
//! User-defined types implement [`DeserializeView`] directly or via the
//! `#[derive(DeserializeView)]` macro (`feature = "derive"`).

use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "std")]
use std::collections::{HashMap, HashSet};
#[cfg(feature = "std")]
use std::hash::{BuildHasher, Hash};

use crate::codec::{Decode, Decoder};
use crate::error::{Result, SerialError};
use crate::traits::Deserialize;

// ---------------------------------------------------------------------------
// Trait + free function
// ---------------------------------------------------------------------------

/// Types that decode by **borrowing** directly from the input slice rather
/// than allocating owned copies.
///
/// `'a` is the lifetime of the underlying byte buffer; every borrowed field
/// in the decoded value points into that buffer. The borrow checker
/// guarantees the value cannot outlive its source.
///
/// # Examples
///
/// Borrow string and byte fields out of a request buffer:
///
/// ```
/// use pack_io::{decode_view, Decode, Decoder, DeserializeView, Result, Serialize, Encode};
///
/// // Pair of borrowed and owned versions for round-tripping.
/// struct OwnedMsg { id: u64, text: String, payload: Vec<u8> }
/// struct ViewMsg<'a> { id: u64, text: &'a str, payload: &'a [u8] }
///
/// impl Serialize for OwnedMsg {
///     fn serialize<E: Encode + ?Sized>(&self, e: &mut E) -> Result<()> {
///         self.id.serialize(e)?; self.text.serialize(e)?; self.payload.serialize(e)
///     }
/// }
/// impl<'a> DeserializeView<'a> for ViewMsg<'a> {
///     fn deserialize_view(d: &mut Decoder<'a>) -> Result<Self> {
///         Ok(ViewMsg {
///             id:      u64::deserialize_view(d)?,
///             text:    <&str>::deserialize_view(d)?,
///             payload: <&[u8]>::deserialize_view(d)?,
///         })
///     }
/// }
///
/// let bytes = pack_io::encode(&OwnedMsg {
///     id: 7,
///     text: "hello".into(),
///     payload: vec![1, 2, 3],
/// }).unwrap();
///
/// let view: ViewMsg<'_> = decode_view(&bytes).unwrap();
/// assert_eq!(view.text, "hello");        // &str borrowed from `bytes`
/// assert_eq!(view.payload, &[1, 2, 3]);  // &[u8] borrowed from `bytes`
/// ```
pub trait DeserializeView<'a>: Sized {
    /// Read a value of `Self` from `decoder`, borrowing from its
    /// underlying input slice where possible.
    ///
    /// # Errors
    ///
    /// Any [`crate::SerialError`] the underlying byte reads surface.
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self>;
}

/// Decode a borrowed value of type `T` from `bytes`, requiring the input to
/// be fully consumed.
///
/// This is the **Tier-1** zero-copy entry point — the symmetric counterpart
/// to [`crate::decode`]. The decoded value borrows from `bytes`; the
/// borrow checker enforces that `bytes` outlives the result.
///
/// # Examples
///
/// ```
/// let bytes = pack_io::encode(&"hello").unwrap();
/// let view: &str = pack_io::decode_view(&bytes).unwrap();
/// assert_eq!(view, "hello");
/// ```
///
/// # Errors
///
/// - Returns [`SerialError::TrailingBytes`] when extra bytes follow the value.
/// - Propagates any [`SerialError`] from the type's [`DeserializeView`] impl.
#[inline]
pub fn decode_view<'a, T: DeserializeView<'a>>(bytes: &'a [u8]) -> Result<T> {
    let mut dec = Decoder::new(bytes);
    let value = T::deserialize_view(&mut dec)?;
    let remaining = dec.remaining();
    if remaining != 0 {
        return Err(SerialError::TrailingBytes { remaining });
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// Borrowed primitives
// ---------------------------------------------------------------------------

impl<'a> DeserializeView<'a> for &'a str {
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        let bytes = decoder.read_length_prefixed_borrowed()?;
        core::str::from_utf8(bytes).map_err(|_| SerialError::InvalidUtf8)
    }
}

impl<'a> DeserializeView<'a> for &'a [u8] {
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        decoder.read_length_prefixed_borrowed()
    }
}

// ---------------------------------------------------------------------------
// Fixed-size scalars: DeserializeView ≡ Deserialize
// ---------------------------------------------------------------------------

macro_rules! view_via_owned {
    ($($t:ty),+ $(,)?) => {
        $(
            impl<'a> DeserializeView<'a> for $t {
                #[inline]
                fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
                    <$t as Deserialize>::deserialize(decoder)
                }
            }
        )+
    };
}

view_via_owned!(
    u8, u16, u32, u64, u128, usize,
    i8, i16, i32, i64, i128, isize,
    bool, f32, f64,
    (),
    String,
);

// ---------------------------------------------------------------------------
// Containers: Option, Result, tuples, arrays — propagate the lifetime
// ---------------------------------------------------------------------------

impl<'a, T: DeserializeView<'a>> DeserializeView<'a> for Option<T> {
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        match decoder.read_byte()? {
            0x00 => Ok(None),
            0x01 => Ok(Some(T::deserialize_view(decoder)?)),
            tag => Err(SerialError::InvalidTag {
                kind: "Option",
                tag,
            }),
        }
    }
}

impl<'a, T: DeserializeView<'a>, E: DeserializeView<'a>> DeserializeView<'a>
    for core::result::Result<T, E>
{
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        match decoder.read_byte()? {
            0x00 => Ok(Ok(T::deserialize_view(decoder)?)),
            0x01 => Ok(Err(E::deserialize_view(decoder)?)),
            tag => Err(SerialError::InvalidTag {
                kind: "Result",
                tag,
            }),
        }
    }
}

impl<'a, T: DeserializeView<'a>, const N: usize> DeserializeView<'a> for [T; N] {
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        let mut out: Vec<T> = Vec::with_capacity(N);
        for _ in 0..N {
            out.push(T::deserialize_view(decoder)?);
        }
        out.try_into().map_err(|_| SerialError::IntegerOutOfRange)
    }
}

impl<'a, T: DeserializeView<'a>> DeserializeView<'a> for Vec<T> {
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        let declared = <Decoder<'a> as crate::Decode>::read_varint_u64(decoder)?;
        let max = <Decoder<'a> as crate::Decode>::max_alloc(decoder) as u64;
        if declared > max {
            return Err(SerialError::InvalidLength {
                declared,
                remaining: 0,
            });
        }
        let len = usize::try_from(declared).map_err(|_| SerialError::IntegerOutOfRange)?;
        let initial = len.min(4096);
        let mut out = Vec::with_capacity(initial);
        for _ in 0..len {
            out.push(T::deserialize_view(decoder)?);
        }
        Ok(out)
    }
}

macro_rules! view_tuple {
    ($($name:ident),+) => {
        impl<'a, $($name: DeserializeView<'a>),+> DeserializeView<'a> for ($($name,)+) {
            #[inline]
            fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
                Ok(( $( $name::deserialize_view(decoder)?, )+ ))
            }
        }
    };
}

view_tuple!(T0);
view_tuple!(T0, T1);
view_tuple!(T0, T1, T2);
view_tuple!(T0, T1, T2, T3);
view_tuple!(T0, T1, T2, T3, T4);
view_tuple!(T0, T1, T2, T3, T4, T5);
view_tuple!(T0, T1, T2, T3, T4, T5, T6);
view_tuple!(T0, T1, T2, T3, T4, T5, T6, T7);
view_tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8);
view_tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9);
view_tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
view_tuple!(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);

// ---------------------------------------------------------------------------
// Map / set collections: the container is allocated, but K / V may borrow.
// ---------------------------------------------------------------------------

impl<'a, K, V> DeserializeView<'a> for BTreeMap<K, V>
where
    K: DeserializeView<'a> + Ord,
    V: DeserializeView<'a>,
{
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        let declared = <Decoder<'a> as crate::Decode>::read_varint_u64(decoder)?;
        let max = <Decoder<'a> as crate::Decode>::max_alloc(decoder) as u64;
        if declared > max {
            return Err(SerialError::InvalidLength {
                declared,
                remaining: 0,
            });
        }
        let len = usize::try_from(declared).map_err(|_| SerialError::IntegerOutOfRange)?;
        let mut out = BTreeMap::new();
        for _ in 0..len {
            let k = K::deserialize_view(decoder)?;
            let v = V::deserialize_view(decoder)?;
            let _ = out.insert(k, v);
        }
        Ok(out)
    }
}

impl<'a, T> DeserializeView<'a> for BTreeSet<T>
where
    T: DeserializeView<'a> + Ord,
{
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        let declared = <Decoder<'a> as crate::Decode>::read_varint_u64(decoder)?;
        let max = <Decoder<'a> as crate::Decode>::max_alloc(decoder) as u64;
        if declared > max {
            return Err(SerialError::InvalidLength {
                declared,
                remaining: 0,
            });
        }
        let len = usize::try_from(declared).map_err(|_| SerialError::IntegerOutOfRange)?;
        let mut out = BTreeSet::new();
        for _ in 0..len {
            let _ = out.insert(T::deserialize_view(decoder)?);
        }
        Ok(out)
    }
}

#[cfg(feature = "std")]
impl<'a, K, V, S> DeserializeView<'a> for HashMap<K, V, S>
where
    K: DeserializeView<'a> + Hash + Eq,
    V: DeserializeView<'a>,
    S: BuildHasher + Default,
{
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        let declared = <Decoder<'a> as crate::Decode>::read_varint_u64(decoder)?;
        let max = <Decoder<'a> as crate::Decode>::max_alloc(decoder) as u64;
        if declared > max {
            return Err(SerialError::InvalidLength {
                declared,
                remaining: 0,
            });
        }
        let len = usize::try_from(declared).map_err(|_| SerialError::IntegerOutOfRange)?;
        let initial = len.min(4096);
        let mut out = HashMap::with_capacity_and_hasher(initial, S::default());
        for _ in 0..len {
            let k = K::deserialize_view(decoder)?;
            let v = V::deserialize_view(decoder)?;
            let _ = out.insert(k, v);
        }
        Ok(out)
    }
}

#[cfg(feature = "std")]
impl<'a, T, S> DeserializeView<'a> for HashSet<T, S>
where
    T: DeserializeView<'a> + Hash + Eq,
    S: BuildHasher + Default,
{
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        let declared = <Decoder<'a> as crate::Decode>::read_varint_u64(decoder)?;
        let max = <Decoder<'a> as crate::Decode>::max_alloc(decoder) as u64;
        if declared > max {
            return Err(SerialError::InvalidLength {
                declared,
                remaining: 0,
            });
        }
        let len = usize::try_from(declared).map_err(|_| SerialError::IntegerOutOfRange)?;
        let initial = len.min(4096);
        let mut out = HashSet::with_capacity_and_hasher(initial, S::default());
        for _ in 0..len {
            let _ = out.insert(T::deserialize_view(decoder)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn borrowed_str_round_trips() {
        let bytes = encode(&"hello").unwrap();
        let view: &str = decode_view(&bytes).unwrap();
        assert_eq!(view, "hello");
    }

    #[test]
    fn borrowed_bytes_round_trip() {
        let bytes = encode(&vec![1u8, 2, 3, 4, 5]).unwrap();
        let view: &[u8] = decode_view(&bytes).unwrap();
        assert_eq!(view, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn primitive_view_decodes_like_owning() {
        let bytes = encode(&42_u64).unwrap();
        let n: u64 = decode_view(&bytes).unwrap();
        assert_eq!(n, 42);
    }

    #[test]
    fn option_borrowed_view_round_trips() {
        let bytes = encode(&Some(String::from("hi"))).unwrap();
        let v: Option<&str> = decode_view(&bytes).unwrap();
        assert_eq!(v, Some("hi"));

        let none_bytes = encode::<Option<String>>(&None).unwrap();
        let v: Option<&str> = decode_view(&none_bytes).unwrap();
        assert_eq!(v, None);
    }

    #[test]
    fn tuple_with_borrowed_str_round_trips() {
        let owned = (7_u64, String::from("hello"), true);
        let bytes = encode(&owned).unwrap();
        let view: (u64, &str, bool) = decode_view(&bytes).unwrap();
        assert_eq!(view, (7, "hello", true));
    }

    #[test]
    fn vec_of_borrowed_str_round_trips() {
        let owned = vec![
            String::from("alpha"),
            String::from("beta"),
            String::from("gamma"),
        ];
        let bytes = encode(&owned).unwrap();
        let view: Vec<&str> = decode_view(&bytes).unwrap();
        assert_eq!(view, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn decode_view_rejects_trailing_bytes() {
        let mut bytes = encode(&"hi").unwrap();
        bytes.push(0xff);
        let err = decode_view::<&str>(&bytes).expect_err("trailing bytes");
        assert!(matches!(err, SerialError::TrailingBytes { remaining: 1 }));
    }

    #[test]
    fn borrowed_str_with_invalid_utf8_is_rejected() {
        let bytes = [0x02u8, 0xff, 0xff];
        let err = decode_view::<&str>(&bytes).expect_err("invalid utf-8");
        assert!(matches!(err, SerialError::InvalidUtf8));
    }

    #[test]
    fn map_with_borrowed_keys_round_trips() {
        let mut owned: BTreeMap<String, u32> = BTreeMap::new();
        let _ = owned.insert("alpha".to_string(), 1);
        let _ = owned.insert("beta".to_string(), 2);
        let bytes = encode(&owned).unwrap();
        let view: BTreeMap<&str, u32> = decode_view(&bytes).unwrap();
        assert_eq!(view.get("alpha"), Some(&1));
        assert_eq!(view.get("beta"), Some(&2));
    }
}
