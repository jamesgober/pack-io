//! The [`Serialize`] and [`Deserialize`] traits.
//!
//! These are the seam between a Rust value and its wire-format bytes. They
//! are generic over the [`Encode`] / [`Decode`] behaviour traits, so a single
//! `impl Serialize` works through both the in-memory [`crate::Encoder`] and
//! the streaming [`crate::IoEncoder`].
//!
//! The built-in primitive and collection implementations live in
//! [`crate::impls`]. User-defined types either implement these traits
//! directly or — with the `derive` feature on — derive them with
//! `#[derive(pack_io::Serialize, pack_io::Deserialize)]`. The derive macro
//! writes a sound, deterministic implementation for any struct or enum
//! whose fields are themselves `Serialize` / `Deserialize`.

use alloc::vec::Vec;

use crate::codec::{Decode, Encode, SizeCounter, decode_elements_counted, initial_capacity};
use crate::error::{Result, SerialError};

/// Types that know how to write themselves into any [`Encode`] sink.
///
/// The contract is: `serialize` appends the type's wire-format bytes to the
/// encoder. It does **not** clear the encoder first — encoders are
/// stream-shaped, and may already hold bytes from prior writes.
///
/// Implementors MUST be deterministic: for any two equal values `a` and `b`
/// (in the type's `Eq` / `PartialEq` sense, or its semantic equivalent for
/// types like `f64`), the bytes appended by `a.serialize(&mut enc)` MUST
/// equal the bytes appended by `b.serialize(&mut enc)`.
///
/// # Examples
///
/// ```
/// use pack_io::{Encode, Encoder, Result, Serialize};
///
/// struct Point { x: i32, y: i32 }
///
/// impl Serialize for Point {
///     fn serialize<E: Encode + ?Sized>(&self, enc: &mut E) -> Result<()> {
///         self.x.serialize(enc)?;
///         self.y.serialize(enc)
///     }
/// }
///
/// let mut enc = Encoder::new();
/// Point { x: 3, y: -7 }.serialize(&mut enc).unwrap();
/// assert!(!enc.as_bytes().is_empty());
/// ```
pub trait Serialize {
    /// Append the encoded bytes of `self` to `encoder`.
    ///
    /// # Errors
    ///
    /// Primitive impls in this crate are infallible on the in-memory
    /// [`crate::Encoder`]. Streaming encoders (`IoEncoder<W>`) may surface
    /// a [`crate::SerialError`] when the underlying `Write` errors. User
    /// impls may also surface custom errors via [`crate::SerialError`].
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()>;

    /// Append the encoded bytes of every element of `slice` to `encoder`.
    ///
    /// The default implementation calls [`Serialize::serialize`] once per
    /// element. Types that can take advantage of a single bulk operation
    /// — most importantly `u8`, which compiles down to a single
    /// `extend_from_slice` / `Write::write_all` instead of N individual
    /// pushes — override this to skip the per-element loop overhead.
    ///
    /// **This method is the seam that makes `Vec<u8>` encode at memcpy
    /// speed without forcing `unsafe` or specialisation onto the public
    /// trait surface.** The `[T]::serialize` impl calls
    /// `T::serialize_slice(self, encoder)` rather than looping inline, so
    /// any `Serialize` impl that overrides `serialize_slice` automatically
    /// applies to every `&[T]`, `Vec<T>`, `[T; N]`, and `&[u8]`-shaped
    /// payload that flows through it.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by the per-element / bulk operation.
    #[inline]
    fn serialize_slice<E: Encode + ?Sized>(slice: &[Self], encoder: &mut E) -> Result<()>
    where
        Self: Sized,
    {
        for item in slice {
            item.serialize(encoder)?;
        }
        Ok(())
    }

    /// Exact number of bytes [`Serialize::serialize`] appends for `self`.
    ///
    /// The default runs `serialize` against a counting sink that stores
    /// nothing, so it is always correct but costs one encode pass. The
    /// built-in impls and the derive macro override it with direct
    /// arithmetic (a `String` is `varint_len(len) + len`, a map sums its
    /// entries without sorting them, …).
    ///
    /// Use it to size a buffer exactly before encoding
    /// (`Encoder::with_capacity(v.encoded_len()?)`), or to write a length
    /// prefix without encoding the value twice. Versioned structs use it to
    /// write their body length up front instead of buffering the body.
    ///
    /// **Overrides MUST return exactly the number of bytes `serialize`
    /// writes.** A versioned struct that contains a value whose override
    /// over- or under-counts produces a corrupt frame.
    ///
    /// # Errors
    ///
    /// Whatever [`Serialize::serialize`] would return for this value.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::{Encoder, Serialize};
    ///
    /// let value = (7_u64, String::from("hello"), vec![1_u32, 2, 300]);
    /// let len = value.encoded_len().unwrap();
    ///
    /// let mut enc = Encoder::with_capacity(len); // exactly one allocation
    /// value.serialize(&mut enc).unwrap();
    /// assert_eq!(enc.as_bytes().len(), len);
    /// ```
    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        let mut counter = SizeCounter::default();
        self.serialize(&mut counter)?;
        Ok(counter.count())
    }

    /// Exact number of bytes [`Serialize::serialize_slice`] appends for
    /// `slice` (no length prefix). Counterpart of
    /// [`Serialize::encoded_len`]; `u8` overrides it to return
    /// `slice.len()` without iterating.
    ///
    /// # Errors
    ///
    /// Whatever the per-element [`Serialize::encoded_len`] returns.
    #[inline]
    fn encoded_len_slice(slice: &[Self]) -> Result<usize>
    where
        Self: Sized,
    {
        slice.iter().try_fold(0usize, |acc, item| {
            Ok(acc.saturating_add(item.encoded_len()?))
        })
    }
}

/// Types that know how to read themselves from any [`Decode`] source.
///
/// The contract is: `deserialize` consumes exactly the bytes that a
/// corresponding `Serialize` would have produced for the returned value, no
/// more and no fewer. On any malformed input it MUST return an error and
/// MUST NOT panic, allocate unboundedly, or read past the decoder's
/// underlying source.
///
/// Hand-written impls for **recursive** types should bracket their body
/// with [`Decode::enter_nested`] / [`Decode::leave_nested`] so that
/// [`crate::Config::max_depth`] bounds the recursion (derived impls do this
/// automatically).
///
/// Round-trip invariant:
/// `decode::<T>(&encode(&v)?)? == v` for every `v: T`.
///
/// # Examples
///
/// ```
/// use pack_io::{Decode, Decoder, Deserialize, Result, encode};
///
/// struct Point { x: i32, y: i32 }
///
/// impl Deserialize for Point {
///     fn deserialize<D: Decode + ?Sized>(dec: &mut D) -> Result<Self> {
///         Ok(Point {
///             x: i32::deserialize(dec)?,
///             y: i32::deserialize(dec)?,
///         })
///     }
/// }
///
/// # impl pack_io::Serialize for Point {
/// #     fn serialize<E: pack_io::Encode + ?Sized>(&self, e: &mut E) -> Result<()> {
/// #         self.x.serialize(e)?;
/// #         self.y.serialize(e)
/// #     }
/// # }
/// let bytes = encode(&Point { x: 3, y: -7 }).unwrap();
/// let mut dec = Decoder::new(&bytes);
/// let back = Point::deserialize(&mut dec).unwrap();
/// assert_eq!((back.x, back.y), (3, -7));
/// ```
pub trait Deserialize: Sized {
    /// Read a value of `Self` from the decoder, advancing its cursor.
    ///
    /// # Errors
    ///
    /// Any [`crate::SerialError`] the underlying byte reads surface
    /// (truncated input, invalid length prefix, hostile varint, …).
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self>;

    /// Read `count` consecutive `Self` values into a freshly-allocated `Vec`.
    ///
    /// The default implementation calls [`Deserialize::deserialize`] in a
    /// loop. Types whose batch read can be done in a single bulk operation
    /// — most importantly `u8`, which compiles down to a single
    /// `Read::read_exact` instead of N individual byte reads — override
    /// this for the memcpy-class fast path.
    ///
    /// **This method is the seam that makes `Vec<u8>` decode at memcpy
    /// speed without forcing `unsafe` or specialisation onto the public
    /// trait surface.** The `Vec<T>::deserialize` impl calls
    /// `T::deserialize_many(decoder, len)` rather than looping inline.
    ///
    /// Implementations MUST NOT size an allocation from `count` alone. The
    /// caller has validated `count` against [`crate::Config::max_alloc`],
    /// but that is a per-value cap, not proof the input holds `count`
    /// elements. The default pre-allocates at most the smaller of `count`,
    /// the decoder's [`Decode::remaining_hint`], and 64 KiB worth of
    /// `Self` (4 KiB when the remaining input is unknown), then grows as
    /// elements actually decode. Elements that consume no input bytes are
    /// charged `size_of::<Self>()` each (at least 1) through
    /// [`Decode::charge_unbacked`], so a count of free elements is bounded
    /// by `max_alloc` across the whole decode.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by the per-element / bulk read, and
    /// [`crate::SerialError::InvalidLength`] when free elements exceed the
    /// budget.
    fn deserialize_many<D: Decode + ?Sized>(decoder: &mut D, count: usize) -> Result<Vec<Self>> {
        let mut out = Vec::with_capacity(initial_capacity::<Self, D>(count, decoder));
        decode_elements_counted::<Self, D, _>(decoder, count, |decoder| {
            out.push(Self::deserialize(decoder)?);
            Ok(())
        })?;
        Ok(out)
    }

    /// Read `N` consecutive `Self` values into an array.
    ///
    /// The default decodes into a temporary `Vec` and converts it (building
    /// an array of a generic `T` element by element needs either `unsafe`
    /// or the unstable `core::array::try_from_fn`). The primitive impls
    /// override it with a stack array: `u8` with a single read, so
    /// `[u8; N]` (hashes, keys, fixed-width identifiers) is one copy, and
    /// the other integers, floats, `bool` and `char` by filling a
    /// zero-initialised array, so none of them touch the heap.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by the per-element / bulk read.
    fn deserialize_array<D: Decode + ?Sized, const N: usize>(decoder: &mut D) -> Result<[Self; N]> {
        let mut out: Vec<Self> = Vec::with_capacity(N);
        for _ in 0..N {
            out.push(Self::deserialize(decoder)?);
        }
        // Exactly `N` elements were pushed above, so the conversion cannot
        // fail; map the impossible case to an error rather than panicking.
        out.try_into()
            .map_err(|_| SerialError::custom("array length mismatch"))
    }
}
