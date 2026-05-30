//! The codec primitives: [`Encoder`], [`Decoder`], [`Config`], and the
//! Tier-1 [`encode`] / [`decode`] free functions.
//!
//! ## Layering
//!
//! - **Tier 1** — the [`encode`] / [`decode`] free functions. One line each
//!   direction, no setup, no type parameters beyond the target type.
//! - **Tier 2** — the [`Encoder`] and [`Decoder`] structs. Use them when you
//!   want to write into a caller-owned buffer (avoiding the per-call
//!   allocation of Tier 1) or stream multiple values from a single buffer.
//! - **Tier 3** — implementing the [`Serialize`] / [`Deserialize`] traits
//!   directly for your own types. The derive macro (`feature = "derive"`)
//!   does this for you in `0.4`.
//!
//! ## Safety contract for decoders
//!
//! Every method on [`Decoder`] is total: it either returns the requested
//! value (advancing the read cursor) or returns a [`SerialError`]. It never
//! panics, never reads past the input slice, and never allocates more memory
//! than the [`Config::max_alloc`] cap permits.

use alloc::vec::Vec;

use crate::error::{Result, SerialError};
use crate::traits::{Deserialize, Serialize};

/// Configuration for a codec session.
///
/// At construction time the codec validates the configuration; an invalid
/// config (currently: `max_alloc == 0`) is rejected before any bytes are read
/// or written. Validation happens once, in [`Decoder::with_config`], not on
/// every operation.
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
/// // value (a `String`, a `Vec<u8>`, …). Hostile producers that send
/// // multi-gigabyte length prefixes fail fast.
/// let cfg = Config::new().with_max_alloc(16 * 1024);
/// let dec = Decoder::with_config(&[], cfg).expect("non-zero cap");
/// drop(dec);
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Maximum number of bytes the decoder may allocate for any single
    /// length-prefixed value (a `String`, a `Vec<u8>`, …).
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
    fn validate(self) -> Result<Self> {
        if self.max_alloc == 0 {
            return Err(SerialError::InvalidLength {
                declared: 0,
                remaining: 0,
            });
        }
        Ok(self)
    }
}

/// Buffered encoder. Writes into an owned `Vec<u8>`; the buffer can be
/// reused across encodes by calling [`Encoder::take`] to swap it out.
///
/// The encoder is intentionally configuration-light — there is nothing on
/// the encode path that needs an allocation cap. The [`Config`] type is
/// consumed by [`Decoder`].
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
    /// implementation. The built-in primitive impls are infallible, so for
    /// those this method never errors.
    #[inline]
    pub fn write<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        value.serialize(self)
    }

    /// Append a raw byte to the buffer. Low-level — used by primitive
    /// `Serialize` impls.
    #[inline]
    pub(crate) fn push_byte(&mut self, byte: u8) {
        self.out.push(byte);
    }

    /// Append a slice of raw bytes to the buffer. Low-level — used by
    /// primitive `Serialize` impls.
    #[inline]
    pub(crate) fn push_bytes(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    /// Append a `u64` as a LEB128 varint.
    #[inline]
    pub(crate) fn write_varint_u64(&mut self, value: u64) {
        let _ = crate::varint::encode_u64(value, &mut self.out);
    }

    /// Append a `u128` as a LEB128 varint.
    #[inline]
    pub(crate) fn write_varint_u128(&mut self, value: u128) {
        let _ = crate::varint::encode_u128(value, &mut self.out);
    }
}

/// Cursored decoder. Borrows from an input slice and advances a position
/// pointer as values are read. Bounds-checked on every operation.
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

    /// Read the next byte, advancing the cursor.
    #[inline]
    pub(crate) fn read_byte(&mut self) -> Result<u8> {
        let byte = match self.input.get(self.pos) {
            Some(b) => *b,
            None => {
                return Err(SerialError::UnexpectedEof {
                    needed: 1,
                    remaining: 0,
                });
            }
        };
        self.pos += 1;
        Ok(byte)
    }

    /// Borrow `count` bytes from the input, advancing the cursor.
    #[inline]
    pub(crate) fn read_slice(&mut self, count: usize) -> Result<&'a [u8]> {
        let remaining = self.remaining();
        if count > remaining {
            return Err(SerialError::UnexpectedEof {
                needed: count,
                remaining,
            });
        }
        let start = self.pos;
        let end = start + count;
        let slice = &self.input[start..end];
        self.pos = end;
        Ok(slice)
    }

    /// Read a LEB128 varint as a `u64`.
    #[inline]
    pub(crate) fn read_varint_u64(&mut self) -> Result<u64> {
        let rest = &self.input[self.pos..];
        let (value, consumed) = crate::varint::decode_u64(rest)?;
        self.pos += consumed;
        Ok(value)
    }

    /// Read a LEB128 varint as a `u128`.
    #[inline]
    pub(crate) fn read_varint_u128(&mut self) -> Result<u128> {
        let rest = &self.input[self.pos..];
        let (value, consumed) = crate::varint::decode_u128(rest)?;
        self.pos += consumed;
        Ok(value)
    }

    /// Read a length-prefixed byte run, returning a borrowed slice.
    ///
    /// The length is read as a varint, validated against `max_alloc`, then
    /// the corresponding number of bytes is borrowed from the input.
    #[inline]
    pub(crate) fn read_length_prefixed(&mut self) -> Result<&'a [u8]> {
        let len_u64 = self.read_varint_u64()?;
        let max = self.config.max_alloc as u64;
        if len_u64 > max {
            return Err(SerialError::InvalidLength {
                declared: len_u64,
                remaining: self.remaining(),
            });
        }
        let len = len_u64 as usize;
        let remaining = self.remaining();
        if len > remaining {
            return Err(SerialError::InvalidLength {
                declared: len_u64,
                remaining,
            });
        }
        self.read_slice(len)
    }
}

/// Encode `value` into a freshly allocated `Vec<u8>`.
///
/// This is the **Tier-1** entry point — the one-line surface for the common
/// case. Allocates one buffer sized exactly to fit the encoded value.
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
/// The built-in primitive impls are infallible, so for those this never
/// errors.
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
        buf.push(0xff); // sentinel - encoder should append after this
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
        assert_eq!(second, [0x02]); // varint(2) is one byte
    }

    #[test]
    fn decode_rejects_trailing_bytes() {
        // u8 varint(7) = 0x07; we append a stray byte
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
        // Length-prefix 5, then 5 bytes of payload.
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
        // Declares 16 bytes but only 2 remain.
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
        let _ = dec.read_slice(2).unwrap();
        assert_eq!(dec.position(), 3);
        assert!(dec.is_empty());
    }
}
