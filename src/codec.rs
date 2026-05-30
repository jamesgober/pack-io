//! The codec primitives: the [`Encode`] / [`Decode`] behaviour traits, the
//! concrete in-memory [`Encoder`] / [`Decoder`] types, the [`Config`] struct,
//! and the Tier-1 [`encode`] / [`decode`] free functions.
//!
//! ## Layering
//!
//! - **Tier 1** — the [`encode`] / [`decode`] free functions. One line each
//!   direction, no setup, no type parameters beyond the target type.
//! - **Tier 2** — concrete encoder / decoder types. The in-memory pair
//!   ([`Encoder`] + [`Decoder`]) lives in this module; the streaming pair
//!   ([`crate::IoEncoder`] + [`crate::IoDecoder`]) lives in
//!   [`crate::io`] and is `std`-gated. All four implement the [`Encode`] /
//!   [`Decode`] behaviour traits, so [`Serialize`] / [`Deserialize`] impls
//!   work through any of them.
//! - **Tier 3** — implementing the [`Serialize`] / [`Deserialize`] traits
//!   directly on your own types. Generic over `E: Encode` / `D: Decode`, so
//!   one impl works for both in-memory and streaming codecs.
//!
//! ## Safety contract for decoders
//!
//! Every method on [`Decode`] is total: it either returns the requested
//! value (advancing the read cursor) or returns a [`SerialError`]. It never
//! panics, never reads past the input, and never allocates more memory than
//! the [`Config::max_alloc`] cap permits.

use alloc::vec;
use alloc::vec::Vec;

use crate::error::{Result, SerialError};
use crate::traits::{Deserialize, Serialize};
use crate::varint;

/// Configuration for a decode session.
///
/// At construction time the codec validates the configuration; an invalid
/// config (currently: `max_alloc == 0`) is rejected before any bytes are read.
/// Validation happens once, in [`Decoder::with_config`] /
/// [`crate::IoDecoder::with_config`], not on every operation.
///
/// `Config` is `#[non_exhaustive]` so the project can add knobs in a MINOR
/// release without breaking downstream code. Build instances with
/// [`Config::new`] / [`Config::with_max_alloc`] or via [`Default`].
///
/// # Examples
///
/// ```
/// use pack_io::{Config, Decoder};
///
/// // Refuse to allocate more than 16 KiB for any single length-prefixed
/// // value (a `String`, a `Vec<u8>`, a collection element count, …).
/// // Hostile producers that send multi-gigabyte length prefixes fail fast.
/// let cfg = Config::new().with_max_alloc(16 * 1024);
/// let dec = Decoder::with_config(&[], cfg).expect("non-zero cap");
/// drop(dec);
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Maximum number of bytes the decoder may allocate for any single
    /// length-prefixed value (a `String`, a `Vec<u8>`, a collection element
    /// count, …).
    ///
    /// The default is 1 GiB, which is enough that well-formed inputs are
    /// never rejected on size, while still defending against the obvious
    /// hostile-length-prefix DoS. Tighten this in any context that accepts
    /// untrusted input from a low-budget producer.
    pub max_alloc: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl Config {
    /// Default configuration: `max_alloc = 1 GiB`.
    ///
    /// 1 GiB is large enough to be irrelevant for well-formed inputs and
    /// small enough to refuse the obvious `length = u64::MAX` attack before
    /// allocating a single byte.
    ///
    /// # Examples
    ///
    /// ```
    /// let cfg = pack_io::Config::new();
    /// assert_eq!(cfg.max_alloc, 1 << 30);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self { max_alloc: 1 << 30 }
    }

    /// Replace `max_alloc` and return the updated config.
    ///
    /// # Examples
    ///
    /// ```
    /// let cfg = pack_io::Config::new().with_max_alloc(4096);
    /// assert_eq!(cfg.max_alloc, 4096);
    /// ```
    #[must_use]
    pub const fn with_max_alloc(mut self, max_alloc: usize) -> Self {
        self.max_alloc = max_alloc;
        self
    }

    /// Validate the configuration. Returns an error if any field is
    /// nonsensical.
    pub(crate) fn validate(self) -> Result<Self> {
        if self.max_alloc == 0 {
            return Err(SerialError::InvalidLength {
                declared: 0,
                remaining: 0,
            });
        }
        Ok(self)
    }
}

// ---------------------------------------------------------------------------
// Encode / Decode behaviour traits
// ---------------------------------------------------------------------------

/// Sink that a [`Serialize`] implementation writes its wire-format bytes
/// into.
///
/// Implemented by every concrete encoder in the crate ([`Encoder`] for the
/// in-memory case, [`crate::IoEncoder`] for `std::io::Write` streams). User
/// code rarely implements `Encode` directly — `Serialize` impls are written
/// generically over `E: Encode` so a single impl works for every encoder
/// flavour.
///
/// # Examples
///
/// ```
/// use pack_io::{Encode, Encoder, Result};
///
/// // A helper that writes a length-prefixed list of `u32`s into any encoder.
/// fn write_u32_list<E: Encode>(enc: &mut E, items: &[u32]) -> Result<()> {
///     enc.write_varint_u64(items.len() as u64)?;
///     for item in items {
///         enc.write_varint_u64(u64::from(*item))?;
///     }
///     Ok(())
/// }
///
/// let mut enc = Encoder::new();
/// write_u32_list(&mut enc, &[1, 2, 3]).unwrap();
/// ```
pub trait Encode {
    /// Append a single byte.
    ///
    /// # Errors
    ///
    /// Returns the encoder's underlying error variant (I/O failure for
    /// streaming encoders; never errors for the in-memory [`Encoder`]).
    fn write_byte(&mut self, byte: u8) -> Result<()>;

    /// Append a slice of bytes.
    ///
    /// # Errors
    ///
    /// Same as [`Encode::write_byte`].
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()>;

    /// Hint that the caller is about to write `additional` more bytes.
    ///
    /// In-memory encoders MAY pre-allocate the requested capacity to avoid
    /// intermediate `Vec` growth. Streaming encoders typically ignore the
    /// hint. The default implementation is a no-op.
    #[inline]
    fn reserve(&mut self, additional: usize) {
        let _ = additional;
    }

    /// Append a `u64` as an unsigned LEB128 varint (1–10 bytes).
    ///
    /// # Errors
    ///
    /// Same as [`Encode::write_bytes`].
    #[inline]
    fn write_varint_u64(&mut self, value: u64) -> Result<()> {
        let mut buf = [0u8; varint::MAX_VARINT_LEN_U64];
        let n = varint::write_u64(value, &mut buf);
        self.write_bytes(&buf[..n])
    }

    /// Append a `u128` as an unsigned LEB128 varint (1–19 bytes).
    ///
    /// # Errors
    ///
    /// Same as [`Encode::write_bytes`].
    #[inline]
    fn write_varint_u128(&mut self, value: u128) -> Result<()> {
        let mut buf = [0u8; varint::MAX_VARINT_LEN_U128];
        let n = varint::write_u128(value, &mut buf);
        self.write_bytes(&buf[..n])
    }
}

/// Source that a [`Deserialize`] implementation reads its wire-format bytes
/// from.
///
/// Implemented by every concrete decoder in the crate ([`Decoder`] for the
/// in-memory case, [`crate::IoDecoder`] for `std::io::Read` streams). User
/// code rarely implements `Decode` directly — `Deserialize` impls are
/// written generically over `D: Decode`.
///
/// All methods are **total**: on any byte sequence they either succeed
/// (advancing the cursor) or return a [`SerialError`]. They never panic,
/// never read past the input, and never allocate more memory than
/// [`Decode::max_alloc`] permits.
pub trait Decode {
    /// Read the next byte, advancing the cursor.
    ///
    /// # Errors
    ///
    /// Returns [`SerialError::UnexpectedEof`] if the input is exhausted.
    /// Streaming decoders MAY return an I/O-flavoured error variant.
    fn read_byte(&mut self) -> Result<u8>;

    /// Fill `out` with exactly `out.len()` bytes, advancing the cursor.
    ///
    /// # Errors
    ///
    /// Returns [`SerialError::UnexpectedEof`] on short read.
    fn read_into(&mut self, out: &mut [u8]) -> Result<()>;

    /// Maximum number of bytes the decoder will allocate for a single
    /// length-prefixed value. Mirrors [`Config::max_alloc`].
    fn max_alloc(&self) -> usize;

    /// Read a LEB128 varint as a `u64`.
    ///
    /// # Errors
    ///
    /// Returns [`SerialError::VarintOverflow`] for an overlong encoding,
    /// or [`SerialError::UnexpectedEof`] for a truncated one.
    #[inline]
    fn read_varint_u64(&mut self) -> Result<u64> {
        let mut result: u64 = 0;
        let mut shift: u32 = 0;
        for consumed in 1..=varint::MAX_VARINT_LEN_U64 {
            let byte = self.read_byte()?;
            // The 10th byte may only set bit 0 — anything else overflows u64.
            if consumed == varint::MAX_VARINT_LEN_U64 && (byte & 0xfe) != 0 {
                return Err(SerialError::VarintOverflow);
            }
            result |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
        }
        Err(SerialError::VarintOverflow)
    }

    /// Read a LEB128 varint as a `u128`.
    ///
    /// # Errors
    ///
    /// See [`Decode::read_varint_u64`].
    #[inline]
    fn read_varint_u128(&mut self) -> Result<u128> {
        let mut result: u128 = 0;
        let mut shift: u32 = 0;
        for consumed in 1..=varint::MAX_VARINT_LEN_U128 {
            let byte = self.read_byte()?;
            // The 19th byte may only set the low two bits.
            if consumed == varint::MAX_VARINT_LEN_U128 && (byte & 0xfc) != 0 {
                return Err(SerialError::VarintOverflow);
            }
            result |= u128::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
        }
        Err(SerialError::VarintOverflow)
    }

    /// Read a length-prefixed byte run, allocating a fresh `Vec<u8>`.
    ///
    /// The length is read as a varint, validated against
    /// [`Decode::max_alloc`], then the corresponding number of bytes is
    /// read from the underlying source.
    ///
    /// # Errors
    ///
    /// - [`SerialError::InvalidLength`] if the prefix exceeds `max_alloc`.
    /// - [`SerialError::UnexpectedEof`] if the source runs out before the
    ///   declared length is satisfied.
    #[inline]
    fn read_length_prefixed(&mut self) -> Result<Vec<u8>> {
        let declared = self.read_varint_u64()?;
        let max = self.max_alloc() as u64;
        if declared > max {
            return Err(SerialError::InvalidLength {
                declared,
                remaining: 0,
            });
        }
        let len = declared as usize;
        let mut buf = vec![0u8; len];
        self.read_into(&mut buf)?;
        Ok(buf)
    }
}

// ---------------------------------------------------------------------------
// In-memory Encoder
// ---------------------------------------------------------------------------

/// In-memory encoder. Writes into an owned `Vec<u8>`; the buffer can be
/// reused across encodes by calling [`Encoder::take`] to swap it out.
///
/// Implements [`Encode`], so [`Serialize`] impls written generically over
/// `E: Encode` work directly through it.
///
/// # Examples
///
/// ```
/// use pack_io::Encoder;
///
/// let mut enc = Encoder::new();
/// enc.write(&7_u64).unwrap();
/// enc.write(&"hello").unwrap();
/// let bytes = enc.into_inner();
/// assert!(bytes.len() > 0);
/// ```
#[derive(Debug, Default)]
pub struct Encoder {
    out: Vec<u8>,
}

impl Encoder {
    /// Construct an encoder with an empty output buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// let enc = pack_io::Encoder::new();
    /// assert!(enc.as_bytes().is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self { out: Vec::new() }
    }

    /// Construct an encoder backed by `buffer`. The encoder appends to the
    /// buffer rather than allocating its own — callers that re-use a single
    /// `Vec<u8>` across many encodes avoid the per-call allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::Encoder;
    ///
    /// let buf = Vec::with_capacity(64);
    /// let mut enc = Encoder::into_buffer(buf);
    /// enc.write(&42_u64).unwrap();
    /// let buf = enc.into_inner();
    /// assert!(!buf.is_empty());
    /// ```
    #[must_use]
    pub fn into_buffer(buffer: Vec<u8>) -> Self {
        Self { out: buffer }
    }

    /// Borrow the encoded bytes accumulated so far.
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.out
    }

    /// Consume the encoder and return its underlying buffer.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> Vec<u8> {
        self.out
    }

    /// Swap the encoder's buffer with a fresh empty one, returning the bytes
    /// written so far. Useful for "encode then send" loops that want to
    /// re-use the encoder.
    #[must_use]
    pub fn take(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.out)
    }

    /// Encode `value`, appending its bytes to the internal buffer.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by the type's [`Serialize`]
    /// implementation. Primitive impls in this crate never error on an
    /// in-memory encoder.
    #[inline]
    pub fn write<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        value.serialize(self)
    }
}

impl Encode for Encoder {
    #[inline]
    fn write_byte(&mut self, byte: u8) -> Result<()> {
        self.out.push(byte);
        Ok(())
    }

    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.extend_from_slice(bytes);
        Ok(())
    }

    #[inline]
    fn reserve(&mut self, additional: usize) {
        self.out.reserve(additional);
    }
}

// ---------------------------------------------------------------------------
// In-memory Decoder
// ---------------------------------------------------------------------------

/// In-memory decoder. Borrows from an input slice and advances a position
/// pointer as values are read. Bounds-checked on every operation.
///
/// Implements [`Decode`], so [`Deserialize`] impls written generically over
/// `D: Decode` work directly through it.
///
/// # Examples
///
/// ```
/// use pack_io::{Encoder, Decoder};
///
/// let mut enc = Encoder::new();
/// enc.write(&7_u64).unwrap();
/// enc.write(&true).unwrap();
/// let bytes = enc.into_inner();
///
/// let mut dec = Decoder::new(&bytes);
/// let n: u64 = dec.read().unwrap();
/// let b: bool = dec.read().unwrap();
/// assert_eq!(n, 7);
/// assert!(b);
/// assert!(dec.is_empty());
/// ```
#[derive(Debug)]
pub struct Decoder<'a> {
    input: &'a [u8],
    pos: usize,
    config: Config,
}

impl<'a> Decoder<'a> {
    /// Construct a decoder over `bytes`.
    #[inline]
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            input: bytes,
            pos: 0,
            config: Config::default(),
        }
    }

    /// Construct a decoder with the supplied configuration.
    ///
    /// # Errors
    ///
    /// Returns [`SerialError::InvalidLength`] if `config.max_alloc == 0`.
    pub fn with_config(bytes: &'a [u8], config: Config) -> Result<Self> {
        Ok(Self {
            input: bytes,
            pos: 0,
            config: config.validate()?,
        })
    }

    /// Bytes consumed so far from the start of the input.
    #[inline]
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Number of bytes remaining in the input.
    #[inline]
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.pos)
    }

    /// True when there are no more bytes to read.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Decode a value of type `T` from the current position.
    ///
    /// # Errors
    ///
    /// Returns any [`SerialError`] surfaced by `T::deserialize`.
    #[inline]
    pub fn read<T: Deserialize>(&mut self) -> Result<T> {
        T::deserialize(self)
    }
}

impl Decode for Decoder<'_> {
    #[inline]
    fn read_byte(&mut self) -> Result<u8> {
        match self.input.get(self.pos) {
            Some(&b) => {
                self.pos += 1;
                Ok(b)
            }
            None => Err(SerialError::UnexpectedEof {
                needed: 1,
                remaining: 0,
            }),
        }
    }

    #[inline]
    fn read_into(&mut self, out: &mut [u8]) -> Result<()> {
        let n = out.len();
        let remaining = self.remaining();
        if n > remaining {
            return Err(SerialError::UnexpectedEof {
                needed: n,
                remaining,
            });
        }
        let start = self.pos;
        let end = start + n;
        out.copy_from_slice(&self.input[start..end]);
        self.pos = end;
        Ok(())
    }

    #[inline]
    fn max_alloc(&self) -> usize {
        self.config.max_alloc
    }

    /// In-memory specialisation: validates length against the actual buffer
    /// length too, not just `max_alloc`. Catches truncated inputs without
    /// allocating.
    #[inline]
    fn read_length_prefixed(&mut self) -> Result<Vec<u8>> {
        let declared = self.read_varint_u64()?;
        let max = self.config.max_alloc as u64;
        if declared > max {
            return Err(SerialError::InvalidLength {
                declared,
                remaining: self.remaining(),
            });
        }
        let len = declared as usize;
        let remaining = self.remaining();
        if len > remaining {
            return Err(SerialError::InvalidLength {
                declared,
                remaining,
            });
        }
        let start = self.pos;
        let end = start + len;
        let slice = &self.input[start..end];
        self.pos = end;
        Ok(slice.to_vec())
    }
}

// ---------------------------------------------------------------------------
// Tier-1 free functions
// ---------------------------------------------------------------------------

/// Encode `value` into a freshly allocated `Vec<u8>`.
///
/// This is the **Tier-1** entry point — the one-line surface for the common
/// case. Allocates one buffer sized to fit the encoded value.
///
/// # Examples
///
/// ```
/// let bytes = pack_io::encode(&42_u64).unwrap();
/// let back: u64 = pack_io::decode(&bytes).unwrap();
/// assert_eq!(back, 42);
/// ```
///
/// # Errors
///
/// Propagates any error returned by the type's [`Serialize`] implementation.
/// The built-in primitive and collection impls never error on an in-memory
/// encoder.
#[inline]
pub fn encode<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let mut enc = Encoder::new();
    value.serialize(&mut enc)?;
    Ok(enc.into_inner())
}

/// Decode a value of type `T` from `bytes`, requiring the input to be fully
/// consumed.
///
/// This is the **Tier-1** entry point — the one-line surface for the common
/// case. After the value has been read, the decoder checks that no bytes
/// remain; trailing input is reported as [`SerialError::TrailingBytes`].
/// Callers that want to read several values from a single buffer should use
/// [`Decoder`] directly.
///
/// # Examples
///
/// ```
/// let bytes = pack_io::encode(&"hello").unwrap();
/// let back: String = pack_io::decode(&bytes).unwrap();
/// assert_eq!(back, "hello");
/// ```
///
/// # Errors
///
/// - Returns [`SerialError::TrailingBytes`] when extra bytes follow the value.
/// - Propagates any [`SerialError`] from the type's [`Deserialize`] impl.
#[inline]
pub fn decode<T: Deserialize>(bytes: &[u8]) -> Result<T> {
    let mut dec = Decoder::new(bytes);
    let value = T::deserialize(&mut dec)?;
    let remaining = dec.remaining();
    if remaining != 0 {
        return Err(SerialError::TrailingBytes { remaining });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_has_one_gib_cap() {
        let cfg = Config::default();
        assert_eq!(cfg.max_alloc, 1 << 30);
    }

    #[test]
    fn decoder_with_zero_cap_is_rejected() {
        let cfg = Config::new().with_max_alloc(0);
        let err = Decoder::with_config(&[], cfg).expect_err("zero cap is invalid");
        assert!(matches!(err, SerialError::InvalidLength { .. }));
    }

    #[test]
    fn encoder_into_buffer_reuses_caller_vec() {
        let mut buf = Vec::with_capacity(64);
        buf.push(0xff);
        let mut enc = Encoder::into_buffer(buf);
        enc.write(&7_u64).unwrap();
        let out = enc.into_inner();
        assert_eq!(out[0], 0xff);
        assert!(out.len() > 1);
    }

    #[test]
    fn encoder_take_returns_buffer_and_resets() {
        let mut enc = Encoder::new();
        enc.write(&1_u64).unwrap();
        let first = enc.take();
        assert!(!first.is_empty());
        assert!(enc.as_bytes().is_empty());

        enc.write(&2_u64).unwrap();
        let second = enc.take();
        assert_eq!(second, [0x02]);
    }

    #[test]
    fn decode_rejects_trailing_bytes() {
        let mut bytes = encode(&7_u8).unwrap();
        bytes.push(0xff);
        let err = decode::<u8>(&bytes).expect_err("trailing bytes should fail");
        assert!(matches!(err, SerialError::TrailingBytes { remaining: 1 }));
    }

    #[test]
    fn decoder_read_past_end_returns_unexpected_eof() {
        let mut dec = Decoder::new(&[0x01]);
        let _: u8 = dec.read().unwrap();
        let err = dec.read::<u8>().expect_err("past end should fail");
        assert!(matches!(err, SerialError::UnexpectedEof { .. }));
    }

    #[test]
    fn decoder_length_prefix_above_cap_is_rejected() {
        let cfg = Config::new().with_max_alloc(4);
        let bytes = [0x05, b'h', b'e', b'l', b'l', b'o'];
        let mut dec = Decoder::with_config(&bytes, cfg).expect("non-zero cap");
        let err = dec
            .read_length_prefixed()
            .expect_err("length > cap should fail");
        assert!(matches!(
            err,
            SerialError::InvalidLength { declared: 5, .. }
        ));
    }

    #[test]
    fn decoder_length_prefix_overflowing_remaining_is_rejected() {
        let bytes = [0x10, b'a', b'b'];
        let mut dec = Decoder::new(&bytes);
        let err = dec
            .read_length_prefixed()
            .expect_err("length > remaining should fail");
        assert!(matches!(err, SerialError::InvalidLength { .. }));
    }

    #[test]
    fn decoder_position_advances_with_reads() {
        let bytes = [0x01, 0x02, 0x03];
        let mut dec = Decoder::new(&bytes);
        assert_eq!(dec.position(), 0);
        let _ = dec.read_byte().unwrap();
        assert_eq!(dec.position(), 1);
        let mut buf = [0u8; 2];
        dec.read_into(&mut buf).unwrap();
        assert_eq!(dec.position(), 3);
        assert!(dec.is_empty());
    }

    #[test]
    fn read_into_short_read_is_rejected() {
        let mut dec = Decoder::new(&[0x01, 0x02]);
        let mut buf = [0u8; 4];
        let err = dec.read_into(&mut buf).expect_err("short read");
        assert!(matches!(err, SerialError::UnexpectedEof { .. }));
    }
}
