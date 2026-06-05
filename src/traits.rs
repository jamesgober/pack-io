//! The [`Serialize`] and [`Deserialize`] traits.
//!
//! These are the seam between a Rust value and its wire-format bytes. They
//! are generic over the [`Encode`] / [`Decode`] behaviour traits, so a single
//! `impl Serialize` works through both the in-memory [`crate::Encoder`] and
//! the streaming [`crate::IoEncoder`].
//!
//! The built-in primitive and collection implementations live in
//! [`crate::impls`]. User-defined types implement these traits directly today;
//! the `derive` macro (lands in `0.4`) writes a sound implementation
//! automatically.

use alloc::vec::Vec;

use crate::codec::{Decode, Encode};
use crate::error::Result;

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
}

/// Types that know how to read themselves from any [`Decode`] source.
///
/// The contract is: `deserialize` consumes exactly the bytes that a
/// corresponding `Serialize` would have produced for the returned value, no
/// more and no fewer. On any malformed input it MUST return an error and
/// MUST NOT panic, allocate unboundedly, or read past the decoder's
/// underlying source.
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
    /// Implementations MUST cap any internal pre-allocation to bound
    /// memory use against hostile length prefixes — the `count` argument
    /// has already been validated against [`crate::Config::max_alloc`] by
    /// the caller, but defensive implementations should still avoid
    /// preallocating the full `count` for collections whose per-element
    /// overhead is large.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by the per-element / bulk read.
    fn deserialize_many<D: Decode + ?Sized>(decoder: &mut D, count: usize) -> Result<Vec<Self>> {
        // Cap initial capacity for the same reason the default `Vec<T>` impl
        // does — hostile counts that pass `guard_element_count` but would
        // blow the heap on a `Vec::with_capacity(count)` of large `T`.
        let initial = count.min(4096);
        let mut out = Vec::with_capacity(initial);
        for _ in 0..count {
            out.push(Self::deserialize(decoder)?);
        }
        Ok(out)
    }
}
