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
//! relationship the caller has with the source bytes. Both enforce the same
//! [`Config`] limits.
//!
//! ## Built-in implementors
//!
//! - `&'a str`, `&'a [u8]`, `Cow<'a, str>`, `Cow<'a, [u8]>` — the headline
//!   zero-copy types (the `Cow`s come back as `Cow::Borrowed`).
//! - Every primitive (`u8` … `u128`, `i8` … `i128`, `usize`, `isize`,
//!   `bool`, `char`, `f32`, `f64`, `()`), `NonZero*`, `Duration`,
//!   `PhantomData<T>` — `DeserializeView` reduces to [`Deserialize`] for
//!   these, no borrow involved.
//! - Owned text: `String`, `Box<str>`, `Rc<str>`, `Arc<str>`; `Box<T>` for
//!   any `T: DeserializeView`.
//! - `Option<T>`, `Result<T, E>`, tuples `(T1, …, Tn)` (arity 1–12),
//!   fixed-size arrays `[T; N]`.
//! - `Vec<T>`, `VecDeque<T>` and the standard `BTreeMap` / `BTreeSet` /
//!   `HashMap` / `HashSet` — these have to allocate the container itself,
//!   but their element / key / value types may still be borrows.
//!
//! User-defined types implement [`DeserializeView`] directly or via the
//! `#[derive(DeserializeView)]` macro (`feature = "derive"`), which also
//! works on types without a lifetime parameter so they can be nested inside
//! borrowing views.

use alloc::borrow::Cow;
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

use crate::codec::{Config, Decode, Decoder, finish, guard_element_count, initial_capacity};
use crate::error::{Result, SerialError};
use crate::impls::KeyOrder;
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

    /// Read `count` consecutive `Self` values into a `Vec`.
    ///
    /// The view-side counterpart of [`Deserialize::deserialize_many`]. The
    /// default decodes one element at a time with a bounded initial
    /// capacity; `u8` overrides it with one bounds check and one copy, so a
    /// `Vec<u8>` field in a view struct is not decoded byte by byte.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by the per-element / bulk read.
    fn deserialize_view_many(decoder: &mut Decoder<'a>, count: usize) -> Result<Vec<Self>> {
        let mut out = Vec::with_capacity(initial_capacity::<Self, Decoder<'a>>(count, decoder));
        for _ in 0..count {
            out.push(Self::deserialize_view(decoder)?);
        }
        Ok(out)
    }

    /// Read `N` consecutive `Self` values into an array. `u8` overrides it
    /// with a single read into a stack array.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by the per-element / bulk read.
    fn deserialize_view_array<const N: usize>(decoder: &mut Decoder<'a>) -> Result<[Self; N]> {
        let mut out: Vec<Self> = Vec::with_capacity(N);
        for _ in 0..N {
            out.push(Self::deserialize_view(decoder)?);
        }
        // Exactly `N` elements were pushed, so this cannot fail.
        out.try_into()
            .map_err(|_| SerialError::custom("array length mismatch"))
    }
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
    finish(&dec, value)
}

/// [`decode_view`] under an explicit [`Config`].
///
/// # Examples
///
/// ```
/// use pack_io::{Config, SerialError, decode_view_with_config, encode};
///
/// let bytes = encode(&"a fairly long string").unwrap();
/// let tight = Config::new().with_max_alloc(8);
/// let err = decode_view_with_config::<&str>(&bytes, tight).unwrap_err();
/// assert!(matches!(err, SerialError::InvalidLength { .. }));
/// ```
///
/// # Errors
///
/// - Any error [`Decoder::with_config`] reports for an invalid `config`.
/// - [`SerialError::TrailingBytes`] when extra bytes follow the value.
/// - Any [`SerialError`] from the type's [`DeserializeView`] impl.
#[inline]
pub fn decode_view_with_config<'a, T: DeserializeView<'a>>(
    bytes: &'a [u8],
    config: Config,
) -> Result<T> {
    let mut dec = Decoder::with_config(bytes, config)?;
    let value = T::deserialize_view(&mut dec)?;
    finish(&dec, value)
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

impl<'a> DeserializeView<'a> for Cow<'a, str> {
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        <&'a str>::deserialize_view(decoder).map(Cow::Borrowed)
    }
}

impl<'a> DeserializeView<'a> for Cow<'a, [u8]> {
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        <&'a [u8]>::deserialize_view(decoder).map(Cow::Borrowed)
    }
}

// ---------------------------------------------------------------------------
// Fixed-size scalars and owned text: DeserializeView ≡ Deserialize
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
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    bool,
    char,
    f32,
    f64,
    (),
    String,
    Box<str>,
    Rc<str>,
    Duration,
    NonZeroU8,
    NonZeroU16,
    NonZeroU32,
    NonZeroU64,
    NonZeroU128,
    NonZeroUsize,
    NonZeroI8,
    NonZeroI16,
    NonZeroI32,
    NonZeroI64,
    NonZeroI128,
    NonZeroIsize,
);

#[cfg(target_has_atomic = "ptr")]
view_via_owned!(Arc<str>);

impl<'a> DeserializeView<'a> for u8 {
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        decoder.read_byte()
    }

    /// One bounds check and one copy for `Vec<u8>` fields in view structs.
    #[inline]
    fn deserialize_view_many(decoder: &mut Decoder<'a>, count: usize) -> Result<Vec<Self>> {
        decoder.read_borrowed(count).map(<[u8]>::to_vec)
    }

    /// `[u8; N]` in a view struct is one read into a stack array.
    #[inline]
    fn deserialize_view_array<const N: usize>(decoder: &mut Decoder<'a>) -> Result<[Self; N]> {
        <u8 as Deserialize>::deserialize_array::<Decoder<'a>, N>(decoder)
    }
}

impl<'a, T: DeserializeView<'a>> DeserializeView<'a> for Box<T> {
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        T::deserialize_view(decoder).map(Box::new)
    }
}

impl<'a, T: ?Sized> DeserializeView<'a> for PhantomData<T> {
    #[inline]
    fn deserialize_view(_decoder: &mut Decoder<'a>) -> Result<Self> {
        Ok(PhantomData)
    }
}

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
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        T::deserialize_view_array::<N>(decoder)
    }
}

impl<'a, T: DeserializeView<'a>> DeserializeView<'a> for Vec<T> {
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count(declared, decoder)?;
        T::deserialize_view_many(decoder, len)
    }
}

impl<'a, T: DeserializeView<'a>> DeserializeView<'a> for VecDeque<T> {
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        Vec::<T>::deserialize_view(decoder).map(VecDeque::from)
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

/// Decode `len` borrowed `(K, V)` entries, enforcing strictly increasing
/// encoded keys in canonical mode.
fn view_entries<'a, K, V, F>(decoder: &mut Decoder<'a>, len: usize, mut insert: F) -> Result<()>
where
    K: DeserializeView<'a>,
    V: DeserializeView<'a>,
    F: FnMut(K, V),
{
    let mut order = decoder
        .config()
        .canonical
        .then(|| KeyOrder::new("map key order"));
    for _ in 0..len {
        let start = decoder.position();
        let k = K::deserialize_view(decoder)?;
        if let Some(order) = order.as_mut() {
            order.check(decoder, start)?;
        }
        let v = V::deserialize_view(decoder)?;
        insert(k, v);
    }
    Ok(())
}

/// Decode `len` borrowed set elements, enforcing strictly increasing
/// encodings in canonical mode.
fn view_elements<'a, T, F>(decoder: &mut Decoder<'a>, len: usize, mut insert: F) -> Result<()>
where
    T: DeserializeView<'a>,
    F: FnMut(T),
{
    let mut order = decoder
        .config()
        .canonical
        .then(|| KeyOrder::new("set element order"));
    for _ in 0..len {
        let start = decoder.position();
        let item = T::deserialize_view(decoder)?;
        if let Some(order) = order.as_mut() {
            order.check(decoder, start)?;
        }
        insert(item);
    }
    Ok(())
}

impl<'a, K, V> DeserializeView<'a> for BTreeMap<K, V>
where
    K: DeserializeView<'a> + Ord,
    V: DeserializeView<'a>,
{
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count(declared, decoder)?;
        let mut out = BTreeMap::new();
        view_entries(decoder, len, |k, v| {
            let _ = out.insert(k, v);
        })?;
        Ok(out)
    }
}

impl<'a, T> DeserializeView<'a> for BTreeSet<T>
where
    T: DeserializeView<'a> + Ord,
{
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count(declared, decoder)?;
        let mut out = BTreeSet::new();
        view_elements(decoder, len, |item| {
            let _ = out.insert(item);
        })?;
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
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count(declared, decoder)?;
        let capacity = initial_capacity::<(K, V), Decoder<'a>>(len, decoder);
        let mut out = HashMap::with_capacity_and_hasher(capacity, S::default());
        view_entries(decoder, len, |k, v| {
            let _ = out.insert(k, v);
        })?;
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
        let declared = decoder.read_varint_u64()?;
        let len = guard_element_count(declared, decoder)?;
        let capacity = initial_capacity::<T, Decoder<'a>>(len, decoder);
        let mut out = HashSet::with_capacity_and_hasher(capacity, S::default());
        view_elements(decoder, len, |item| {
            let _ = out.insert(item);
        })?;
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
