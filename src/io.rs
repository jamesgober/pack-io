//! `std::io::Read` / `std::io::Write` integration: the streaming Tier-2
//! encoder and decoder pair, plus convenience free functions.
//!
//! This module is gated on the `std` feature (on by default). With `std` off,
//! the crate compiles for `no_std` targets using `core` + `alloc` only and
//! none of this module is reachable.
//!
//! ## When to use which entry point
//!
//! - For one-shot send / receive of a single value, prefer [`encode_into`] /
//!   [`decode_from`]: they take any `Write` / `Read` and handle the
//!   buffering.
//! - For interleaved writes / reads across many values without per-value
//!   allocation, instantiate an [`IoEncoder`] / [`IoDecoder`] and call
//!   `write` / `read` repeatedly.
//!
//! ## Errors
//!
//! Both directions surface I/O failure through the codec's
//! [`crate::SerialError`] type via [`SerialError::Io`]. The `Io` variant
//! captures `std::io::ErrorKind` and a message string — enough to surface
//! the original cause without taking on a non-`Clone` payload.

use std::io::{Read, Write};

use crate::codec::{Config, Decode, Encode};
use crate::error::{Result, SerialError};
use crate::traits::{Deserialize, Serialize};

/// Streaming encoder that writes directly into any [`Write`]-shaped sink.
///
/// Each [`IoEncoder::write`] call calls into the underlying writer for the
/// bytes the type produces. The encoder does **not** buffer; if you wrap a
/// raw socket / file, wrap it in a [`std::io::BufWriter`] first.
///
/// # Examples
///
/// ```
/// use pack_io::IoEncoder;
///
/// let mut sink: Vec<u8> = Vec::new();
/// let mut enc = IoEncoder::new(&mut sink);
/// enc.write(&42_u64).unwrap();
/// enc.write(&"hello").unwrap();
/// assert!(!sink.is_empty());
/// ```
#[derive(Debug)]
pub struct IoEncoder<W: Write> {
    writer: W,
}

impl<W: Write> IoEncoder<W> {
    /// Wrap `writer` in an encoder.
    ///
    /// The encoder does not buffer; wrap raw sockets / files in a
    /// [`std::io::BufWriter`] first if syscall amplification is a concern.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::IoEncoder;
    ///
    /// let mut sink: Vec<u8> = Vec::new();
    /// let _enc = IoEncoder::new(&mut sink);
    /// ```
    #[must_use]
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Borrow the underlying writer.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::IoEncoder;
    ///
    /// let mut sink: Vec<u8> = Vec::new();
    /// let enc = IoEncoder::new(&mut sink);
    /// let _: &&mut Vec<u8> = &enc.writer();
    /// ```
    #[must_use]
    pub fn writer(&self) -> &W {
        &self.writer
    }

    /// Borrow the underlying writer mutably.
    ///
    /// Useful when downstream code needs `&mut W` to call writer-specific
    /// methods (e.g. `flush`) without consuming the encoder.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::{BufWriter, Write};
    /// use pack_io::IoEncoder;
    ///
    /// let mut sink: Vec<u8> = Vec::new();
    /// let buffered = BufWriter::new(&mut sink);
    /// let mut enc = IoEncoder::new(buffered);
    /// enc.write(&7_u64).unwrap();
    /// enc.writer_mut().flush().unwrap();
    /// ```
    #[must_use]
    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Consume the encoder and return the underlying writer.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::IoEncoder;
    ///
    /// let sink: Vec<u8> = Vec::new();
    /// let mut enc = IoEncoder::new(sink);
    /// enc.write(&42_u64).unwrap();
    /// let written: Vec<u8> = enc.into_inner();
    /// assert_eq!(written, &[0x2a]);
    /// ```
    #[must_use]
    pub fn into_inner(self) -> W {
        self.writer
    }

    /// Encode `value` straight into the underlying writer.
    ///
    /// # Errors
    ///
    /// - Propagates any [`crate::SerialError`] from the type's [`Serialize`].
    /// - Maps any `std::io::Error` from the writer into [`SerialError::Io`].
    #[inline]
    pub fn write<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        value.serialize(self)
    }
}

impl<W: Write> Encode for IoEncoder<W> {
    #[inline]
    fn write_byte(&mut self, byte: u8) -> Result<()> {
        self.writer.write_all(&[byte]).map_err(map_io_error)
    }

    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.write_all(bytes).map_err(map_io_error)
    }
}

/// Streaming decoder that reads directly from any [`Read`]-shaped source.
///
/// Each [`IoDecoder::read`] call may issue many small reads against the
/// underlying source. Wrap raw sockets / files in [`std::io::BufReader`]
/// first if read-syscall amplification is a concern.
///
/// # Examples
///
/// ```
/// use pack_io::{IoEncoder, IoDecoder};
/// use std::io::Cursor;
///
/// let mut buf: Vec<u8> = Vec::new();
/// {
///     let mut enc = IoEncoder::new(&mut buf);
///     enc.write(&42_u64).unwrap();
///     enc.write(&"hi").unwrap();
/// }
///
/// let mut dec = IoDecoder::new(Cursor::new(buf));
/// let n: u64 = dec.read().unwrap();
/// let s: String = dec.read().unwrap();
/// assert_eq!((n, s.as_str()), (42, "hi"));
/// ```
#[derive(Debug)]
pub struct IoDecoder<R: Read> {
    reader: R,
    config: Config,
}

impl<R: Read> IoDecoder<R> {
    /// Wrap `reader` with the default [`Config`] (1 GiB `max_alloc`).
    ///
    /// For tighter allocation caps on untrusted input, use
    /// [`IoDecoder::with_config`] instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Cursor;
    /// use pack_io::IoDecoder;
    ///
    /// let bytes = pack_io::encode(&42_u64).unwrap();
    /// let mut dec = IoDecoder::new(Cursor::new(bytes));
    /// let n: u64 = dec.read().unwrap();
    /// assert_eq!(n, 42);
    /// ```
    #[must_use]
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            config: Config::default(),
        }
    }

    /// Wrap `reader` with the supplied configuration.
    ///
    /// # Errors
    ///
    /// Returns [`SerialError::InvalidLength`] if `config.max_alloc == 0`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Cursor;
    /// use pack_io::{Config, IoDecoder};
    ///
    /// let cfg = Config::new().with_max_alloc(16 * 1024);
    /// let bytes = pack_io::encode(&"hello").unwrap();
    /// let mut dec = IoDecoder::with_config(Cursor::new(bytes), cfg).unwrap();
    /// let s: String = dec.read().unwrap();
    /// assert_eq!(s, "hello");
    /// ```
    pub fn with_config(reader: R, config: Config) -> Result<Self> {
        Ok(Self {
            reader,
            config: config.validate()?,
        })
    }

    /// Borrow the underlying reader.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Cursor;
    /// use pack_io::IoDecoder;
    ///
    /// let dec = IoDecoder::new(Cursor::new(vec![0u8; 4]));
    /// assert_eq!(dec.reader().get_ref().len(), 4);
    /// ```
    #[must_use]
    pub fn reader(&self) -> &R {
        &self.reader
    }

    /// Consume the decoder and return the underlying reader.
    ///
    /// Useful when the caller wants to take back ownership of the source
    /// (e.g. to drop the reader, return it to a pool, or feed it to a
    /// different consumer) after the decoded prefix has been processed.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io::Cursor;
    /// use pack_io::IoDecoder;
    ///
    /// let bytes = pack_io::encode(&42_u64).unwrap();
    /// let mut dec = IoDecoder::new(Cursor::new(bytes));
    /// let _n: u64 = dec.read().unwrap();
    /// let reader: Cursor<Vec<u8>> = dec.into_inner();
    /// assert_eq!(reader.position(), 1); // one byte consumed for u64=42
    /// ```
    #[must_use]
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Decode the next value from the underlying reader.
    ///
    /// # Errors
    ///
    /// - Propagates any [`crate::SerialError`] from the type's
    ///   [`Deserialize`].
    /// - Maps any `std::io::Error` from the reader into [`SerialError::Io`].
    #[inline]
    pub fn read<T: Deserialize>(&mut self) -> Result<T> {
        T::deserialize(self)
    }
}

impl<R: Read> Decode for IoDecoder<R> {
    fn read_byte(&mut self) -> Result<u8> {
        let mut buf = [0u8; 1];
        self.read_into(&mut buf)?;
        Ok(buf[0])
    }

    fn read_into(&mut self, out: &mut [u8]) -> Result<()> {
        self.reader.read_exact(out).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                SerialError::UnexpectedEof {
                    needed: out.len(),
                    remaining: 0,
                }
            } else {
                map_io_error(e)
            }
        })
    }

    fn max_alloc(&self) -> usize {
        self.config.max_alloc
    }
}

/// Encode `value` and write the result into `writer` in a single call.
///
/// # Errors
///
/// - Propagates any [`crate::SerialError`] from the type's [`Serialize`].
/// - Maps any `std::io::Error` from the writer into [`SerialError::Io`].
///
/// # Examples
///
/// ```
/// use pack_io::encode_into;
///
/// let mut buf: Vec<u8> = Vec::new();
/// encode_into(&(7_u64, "hello"), &mut buf).unwrap();
/// assert!(!buf.is_empty());
/// ```
#[inline]
pub fn encode_into<T, W>(value: &T, writer: &mut W) -> Result<()>
where
    T: Serialize + ?Sized,
    W: Write,
{
    let mut enc = IoEncoder::new(writer);
    enc.write(value)
}

/// Read all remaining bytes from `reader` and decode them as a single value
/// of type `T`.
///
/// Use this for whole-buffer reads (a length-prefixed message you have
/// already extracted from the transport, a small config file, …). For
/// length-framed protocols where the producer wrote one value and then more
/// bytes for something else, prefer [`IoDecoder`] directly.
///
/// # Errors
///
/// - Returns [`SerialError::TrailingBytes`] if the reader yielded extra
///   bytes after the value was decoded.
/// - Propagates any [`crate::SerialError`] from the type's [`Deserialize`].
/// - Maps any `std::io::Error` from the reader into [`SerialError::Io`].
///
/// # Examples
///
/// ```
/// use pack_io::{encode, decode_from};
/// use std::io::Cursor;
///
/// let bytes = encode(&42_u64).unwrap();
/// let n: u64 = decode_from(&mut Cursor::new(bytes)).unwrap();
/// assert_eq!(n, 42);
/// ```
pub fn decode_from<T, R>(reader: &mut R) -> Result<T>
where
    T: Deserialize,
    R: Read,
{
    let mut buf = alloc::vec::Vec::new();
    let _ = reader.read_to_end(&mut buf).map_err(map_io_error)?;
    crate::decode(&buf)
}

/// Map a `std::io::Error` into [`SerialError::Io`].
#[inline]
fn map_io_error(err: std::io::Error) -> SerialError {
    use alloc::string::ToString;
    SerialError::Io {
        kind: err.kind(),
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode;
    use alloc::vec::Vec;
    use std::io::Cursor;

    #[test]
    fn io_encoder_decoder_round_trip() {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut enc = IoEncoder::new(&mut buf);
            enc.write(&42_u64).unwrap();
            enc.write(&"hello").unwrap();
            enc.write(&true).unwrap();
        }
        let mut dec = IoDecoder::new(Cursor::new(buf));
        let n: u64 = dec.read().unwrap();
        let s: String = dec.read().unwrap();
        let b: bool = dec.read().unwrap();
        assert_eq!((n, s.as_str(), b), (42, "hello", true));
    }

    #[test]
    fn encode_into_writes_same_bytes_as_encode() {
        let value = (1u32, String::from("hi"), -2i32);
        let from_fn = encode(&value).unwrap();
        let mut from_io: Vec<u8> = Vec::new();
        encode_into(&value, &mut from_io).unwrap();
        assert_eq!(from_fn, from_io);
    }

    #[test]
    fn decode_from_reads_same_value_as_decode() {
        let bytes = encode(&(7u64, true)).unwrap();
        let value: (u64, bool) = decode_from(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(value, (7, true));
    }

    #[test]
    fn io_decoder_with_zero_cap_is_rejected() {
        let cfg = Config::new().with_max_alloc(0);
        let bytes: Vec<u8> = Vec::new();
        let err = IoDecoder::with_config(Cursor::new(bytes), cfg).expect_err("zero cap");
        assert!(matches!(err, SerialError::InvalidLength { .. }));
    }

    #[test]
    fn io_decoder_short_read_surfaces_unexpected_eof() {
        // Two-byte varint that says "more coming" but there's nothing after.
        let bytes = alloc::vec![0x80];
        let mut dec = IoDecoder::new(Cursor::new(bytes));
        let err = dec.read::<u64>().expect_err("truncated");
        assert!(matches!(err, SerialError::UnexpectedEof { .. }));
    }
}
