//! The [`Serialize`] and [`Deserialize`] traits.
//!
//! These are the seam between a Rust value and its wire-format bytes. The
//! built-in primitive implementations live in [`crate::impls`]. User-defined
//! types implement these traits directly today; the `derive` macro (lands in
//! `0.4`) writes a sound implementation automatically.

use crate::codec::{Decoder, Encoder};
use crate::error::Result;

/// Types that know how to write themselves into an [`Encoder`].
///
/// The contract is: `serialize` appends the type's wire-format bytes to the
/// encoder's buffer. It does **not** clear the buffer first — encoders are
/// stream-shaped, and an `Encoder` may already hold bytes from prior writes.
///
/// Implementors MUST be deterministic: for any two equal values `a` and `b`
/// (in the type's `Eq` / `PartialEq` sense, or its semantic equivalent for
/// types like `f64`), the bytes appended by `a.serialize(&mut enc)` MUST
/// equal the bytes appended by `b.serialize(&mut enc)`.
///
/// # Examples
///
/// ```
/// use pack_io::{Encoder, Serialize};
///
/// let mut enc = Encoder::new();
/// 42_u64.serialize(&mut enc).unwrap();
/// assert_eq!(enc.as_bytes(), &[42]); // varint(42) is one byte
/// ```
pub trait Serialize {
    /// Append the encoded bytes of `self` to `encoder`.
    ///
    /// # Errors
    ///
    /// Primitive impls in this crate are infallible. User-defined impls may
    /// surface a [`crate::SerialError`] when a type cannot be represented in
    /// the wire format.
    fn serialize(&self, encoder: &mut Encoder) -> Result<()>;
}

/// Types that know how to read themselves from a [`Decoder`].
///
/// The contract is: `deserialize` consumes exactly the bytes that the
/// corresponding `Serialize` would have produced for the returned value, no
/// more and no fewer. On any malformed input it MUST return an error and
/// MUST NOT panic, allocate unboundedly, or read past the decoder's
/// underlying buffer.
///
/// Round-trip invariant:
/// `decode::<T>(&encode(&v)?)? == v` for every `v: T`.
///
/// # Examples
///
/// ```
/// use pack_io::{Decoder, Deserialize, encode};
///
/// let bytes = encode(&7_u64).unwrap();
/// let mut dec = Decoder::new(&bytes);
/// let back = u64::deserialize(&mut dec).unwrap();
/// assert_eq!(back, 7);
/// ```
pub trait Deserialize: Sized {
    /// Read a value of `Self` from the decoder, advancing its read cursor.
    ///
    /// # Errors
    ///
    /// Any [`crate::SerialError`] the underlying byte reads surface
    /// (truncated input, invalid length prefix, hostile varint, …).
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self>;
}
