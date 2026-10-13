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
//! panics and never reads past the input. Allocation is bounded twice over:
//! no single length-prefixed value may exceed [`Config::max_alloc`], and no
//! allocation is sized from a declared length before the bytes backing it
//! are known to exist (in-memory decoders check the remaining input;
//! streaming decoders grow their buffers only as data actually arrives).
//! Nesting is bounded by [`Config::max_depth`].

use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::error::{Result, SerialError};
use crate::traits::{Deserialize, Serialize};
use crate::varint;

/// Configuration for a decode session.
///
/// At construction time the codec validates the configuration; an invalid
/// config (`max_alloc == 0`, `max_depth == 0`) is rejected before any bytes
/// are read. Validation happens once, in [`Decoder::with_config`] /
/// [`crate::IoDecoder::with_config`] / the `*_with_config` free functions,
/// not on every operation.
///
/// `Config` is `#[non_exhaustive]` so the project can add knobs in a MINOR
/// release without breaking downstream code. Build instances with
/// [`Config::new`] and the `with_*` builders, or via [`Default`].
///
/// | Knob | Default | Purpose |
/// |---|---|---|
/// | [`max_alloc`](Config::max_alloc) | 1 GiB | cap on any single length-prefixed value |
/// | [`max_depth`](Config::max_depth) | 128 | cap on nested derived types (stack-overflow guard) |
/// | [`canonical`](Config::canonical) | `false` | reject every non-canonical encoding |
///
/// # Examples
///
/// ```
/// use pack_io::{Config, Decoder};
///
/// // Refuse to allocate more than 16 KiB for any single length-prefixed
/// // value (a `String`, a `Vec<u8>`, a collection element count, …),
/// // refuse nesting deeper than 32 derived types, and only accept the
/// // exact bytes the encoder would have produced.
/// let cfg = Config::new()
///     .with_max_alloc(16 * 1024)
///     .with_max_depth(32)
///     .with_canonical(true);
/// let dec = Decoder::with_config(&[], cfg).expect("valid config");
/// assert_eq!(dec.config(), cfg);
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Maximum number of bytes the decoder may allocate for any single
    /// length-prefixed value (a `String`, a `Vec<u8>`, a collection element
    /// count, a versioned struct body, …).
    ///
    /// The default is 1 GiB, which is enough that well-formed inputs are
    /// never rejected on size, while still defending against the obvious
    /// hostile-length-prefix DoS. Independently of this cap, decoders never
    /// allocate more than the input can actually back (see the module
    /// docs). Tighten this in any context that accepts untrusted input from
    /// a low-budget producer.
    pub max_alloc: usize,

    /// Maximum nesting depth of derived types during one decode.
    ///
    /// Every `#[derive(Deserialize)]` / `#[derive(DeserializeView)]` value
    /// (struct or enum) counts as one level while its fields are being
    /// decoded. The limit exists so that a few kilobytes of hostile input
    /// for a recursive type cannot exhaust the thread's stack, which aborts
    /// the process rather than returning an error. Exceeding it returns
    /// [`SerialError::DepthLimitExceeded`].
    ///
    /// The default of 128 matches `serde_json` and leaves ample headroom on
    /// a 1 MiB stack in debug builds. Built-in container types (`Vec`,
    /// `Option`, maps, tuples, …) do not count: they cannot recurse without
    /// passing through a user type.
    pub max_depth: usize,

    /// Strict canonical decoding. Off by default.
    ///
    /// When `true`, the decoder accepts only the exact bytes the pack-io
    /// encoder would produce for the decoded value, and rejects everything
    /// else with [`SerialError::NonCanonical`]:
    ///
    /// - varints with redundant zero high bytes (`[0x80, 0x00]` for `0`);
    /// - map and set entries that are not in strictly increasing
    ///   encoded-key order (this also rejects duplicates);
    /// - unread bytes inside a versioned struct body whose version is not
    ///   newer than the type the decoder knows.
    ///
    /// Turn it on wherever the encoded bytes are hashed, signed, or used as
    /// a content address. The default stays lenient because the wire
    /// specification permits lenient readers (`docs/WIRE_FORMAT.md` §2.1,
    /// §4.1) and 1.0 decoders accepted these inputs.
    ///
    /// Canonical mode needs to see the raw input, so it is supported by the
    /// in-memory [`Decoder`] (and every free function built on it).
    /// [`crate::IoDecoder::with_config`] rejects it with
    /// [`SerialError::InvalidConfig`]; read the frame into memory first.
    pub canonical: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl Config {
    /// Default value of [`Config::max_alloc`]: 1 GiB.
    pub const DEFAULT_MAX_ALLOC: usize = 1 << 30;

    /// Default value of [`Config::max_depth`]: 128 nested derived types.
    pub const DEFAULT_MAX_DEPTH: usize = 128;

    /// Default configuration: `max_alloc = 1 GiB`, `max_depth = 128`,
    /// lenient (non-canonical) decoding.
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
    /// assert_eq!(cfg.max_depth, 128);
    /// assert!(!cfg.canonical);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_alloc: Self::DEFAULT_MAX_ALLOC,
            max_depth: Self::DEFAULT_MAX_DEPTH,
            canonical: false,
        }
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

    /// Replace `max_depth` and return the updated config.
    ///
    /// `0` is rejected at decoder construction with
    /// [`SerialError::InvalidConfig`].
    ///
    /// # Examples
    ///
    /// ```
    /// let cfg = pack_io::Config::new().with_max_depth(16);
    /// assert_eq!(cfg.max_depth, 16);
    /// ```
    #[must_use]
    pub const fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Turn strict canonical decoding on or off and return the updated
    /// config. See [`Config::canonical`] for exactly what is rejected.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::{Config, SerialError, decode_with_config};
    ///
    /// // `[0x80, 0x00]` is an overlong (two-byte) encoding of 0.
    /// let overlong = [0x80, 0x00];
    /// assert_eq!(pack_io::decode::<u64>(&overlong).unwrap(), 0);
    ///
    /// let strict = Config::new().with_canonical(true);
    /// let err = decode_with_config::<u64>(&overlong, strict).unwrap_err();
    /// assert!(matches!(err, SerialError::NonCanonical { reason: "varint" }));
    /// ```
    #[must_use]
    pub const fn with_canonical(mut self, canonical: bool) -> Self {
        self.canonical = canonical;
        self
    }

    /// Validate the configuration. Returns an error if any field is
    /// nonsensical.
    ///
    /// A zero `max_alloc` keeps reporting `InvalidLength { declared: 0, .. }`
    /// as it did in 1.0, so existing error handling keeps matching; newer
    /// knobs report [`SerialError::InvalidConfig`].
    pub(crate) fn validate(self) -> Result<Self> {
        if self.max_alloc == 0 {
            return Err(SerialError::InvalidLength {
                declared: 0,
                remaining: 0,
            });
        }
        if self.max_depth == 0 {
            return Err(SerialError::InvalidConfig {
                reason: "max_depth must be at least 1",
            });
        }
        Ok(self)
    }
}

/// Upper bound on the bytes a decoder pre-allocates for a collection before
/// any element has been read. Collections larger than this grow while they
/// decode, so honest payloads pay at most a few reallocations while a
/// hostile element count cannot reserve more than this up front.
pub(crate) const PREALLOC_BYTES: usize = 64 * 1024;

/// Initial capacity for a collection of `count` elements of `T`.
///
/// Bounded three ways: by the declared count, by the number of input bytes
/// that remain (when the decoder knows it; every element that needs storage
/// almost always needs at least one byte of input), and by
/// [`PREALLOC_BYTES`] worth of `T`.
#[inline]
pub(crate) fn initial_capacity<T, D: Decode + ?Sized>(count: usize, decoder: &D) -> usize {
    let per_element = core::mem::size_of::<T>().max(1);
    let by_bytes = PREALLOC_BYTES / per_element;
    let by_input = decoder.remaining_hint().unwrap_or(usize::MAX);
    count.min(by_bytes).min(by_input)
}

/// Validate a declared element count against the decoder's `max_alloc`
/// and convert it to `usize`.
#[inline]
pub(crate) fn guard_element_count<D: Decode + ?Sized>(declared: u64, decoder: &D) -> Result<usize> {
    let max = decoder.max_alloc() as u64;
    if declared > max {
        return Err(SerialError::InvalidLength {
            declared,
            remaining: decoder.remaining_hint().unwrap_or(0),
        });
    }
    usize::try_from(declared).map_err(|_| SerialError::IntegerOutOfRange)
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
        // Fast path for the overwhelmingly common case: value fits in a
        // single byte. Skips the stack buffer + write_bytes round-trip.
        if value < 0x80 {
            return self.write_byte(value as u8);
        }
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
/// (advancing the cursor) or return a [`SerialError`]. They never panic and
/// never read past the input. The provided methods never allocate more
/// than [`Decode::max_alloc`] for one value, and never size an allocation
/// from a declared length beyond what [`Decode::remaining_hint`] says the
/// input can supply.
///
/// # Implementing `Decode`
///
/// Only [`read_byte`](Decode::read_byte), [`read_into`](Decode::read_into)
/// and [`max_alloc`](Decode::max_alloc) are required. The other methods
/// have correct, bounded defaults. A custom decoder should additionally
/// override:
///
/// - [`remaining_hint`](Decode::remaining_hint) when it knows how much
///   input is left, so collection pre-allocation is bounded by it;
/// - [`config`](Decode::config), [`depth`](Decode::depth),
///   [`enter_nested`](Decode::enter_nested) and
///   [`leave_nested`](Decode::leave_nested) to enforce
///   [`Config::max_depth`] (the defaults do not track depth);
/// - [`consumed`](Decode::consumed) if it keeps the input in memory, so
///   canonical mode can check map key order.
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

    /// An upper bound on the number of bytes that can still be read, when
    /// the decoder knows it.
    ///
    /// In-memory decoders return `Some(remaining)`. Streaming decoders
    /// return `None` (the default): the source length is unknown. Decode
    /// impls use the hint to refuse, before allocating, any declared length
    /// the input cannot possibly back.
    #[inline]
    fn remaining_hint(&self) -> Option<usize> {
        None
    }

    /// The configuration this decoder enforces.
    ///
    /// The default reports [`Config::default`] with this decoder's
    /// [`max_alloc`](Decode::max_alloc). Decoders built by this crate
    /// return the exact config they were constructed with.
    #[inline]
    fn config(&self) -> Config {
        Config::new().with_max_alloc(self.max_alloc())
    }

    /// Current nesting depth: the number of derived values whose fields are
    /// being decoded right now. Always `0` for decoders that do not track
    /// depth (the default).
    #[inline]
    fn depth(&self) -> usize {
        0
    }

    /// Record entry into one more level of nesting.
    ///
    /// Derived `Deserialize` / `DeserializeView` impls call this before
    /// decoding their fields and [`leave_nested`](Decode::leave_nested)
    /// afterwards. Hand-written impls for **recursive** types should do the
    /// same. The default implementation does not track depth.
    ///
    /// # Errors
    ///
    /// [`SerialError::DepthLimitExceeded`] when the decoder is already at
    /// [`Config::max_depth`].
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::{Decode, Deserialize, Result};
    ///
    /// // A hand-written recursive type: a linked list of bytes.
    /// struct List(Option<(u8, Vec<List>)>);
    ///
    /// impl Deserialize for List {
    ///     fn deserialize<D: Decode + ?Sized>(dec: &mut D) -> Result<Self> {
    ///         dec.enter_nested()?;
    ///         let inner = Option::<(u8, Vec<List>)>::deserialize(dec);
    ///         dec.leave_nested();
    ///         Ok(List(inner?))
    ///     }
    /// }
    ///
    /// // 10,000 levels of `Some((0, [ ... ]))` from 30 KB of input: refused
    /// // with an error instead of overflowing the stack.
    /// let mut hostile = Vec::new();
    /// for _ in 0..10_000 { hostile.extend_from_slice(&[1, 0, 1]); }
    /// hostile.push(0);
    /// let err = pack_io::decode::<List>(&hostile).err().unwrap();
    /// assert!(matches!(err, pack_io::SerialError::DepthLimitExceeded { .. }));
    /// ```
    #[inline]
    fn enter_nested(&mut self) -> Result<()> {
        Ok(())
    }

    /// Record exit from one level of nesting. Pairs with
    /// [`enter_nested`](Decode::enter_nested); call it on the error path
    /// too, so the decoder stays usable after a failed read.
    #[inline]
    fn leave_nested(&mut self) {}

    /// The bytes this decoder has consumed so far, when it keeps its input
    /// in memory.
    ///
    /// The in-memory [`Decoder`] returns `Some(&input[..position])`;
    /// streaming decoders return `None` (the default). Canonical mode uses
    /// it to compare the raw encoded keys of map and set entries.
    #[inline]
    fn consumed(&self) -> Option<&[u8]> {
        None
    }

    /// Read exactly `len` bytes.
    ///
    /// In-memory decoders return a borrow of their input
    /// ([`Cow::Borrowed`], no copy). The default implementation reads into
    /// a fresh buffer that grows in bounded steps as the bytes actually
    /// arrive, so a large declared `len` over a short or stalled stream
    /// costs at most a small first chunk, not `len` bytes. When
    /// [`remaining_hint`](Decode::remaining_hint) is known, a `len` beyond
    /// it is refused before anything is allocated.
    ///
    /// This method does not check `len` against
    /// [`max_alloc`](Decode::max_alloc); callers that took `len` from the
    /// wire must do that first (see
    /// [`read_length_prefixed`](Decode::read_length_prefixed)).
    ///
    /// # Errors
    ///
    /// [`SerialError::UnexpectedEof`] if the input ends first.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::{Decode, Decoder};
    ///
    /// let mut dec = Decoder::new(&[1, 2, 3, 4]);
    /// assert_eq!(&*dec.read_byte_cow(3).unwrap(), &[1, 2, 3]);
    /// assert!(dec.read_byte_cow(2).is_err()); // only one byte left
    /// ```
    fn read_byte_cow(&mut self, len: usize) -> Result<Cow<'_, [u8]>> {
        read_bytes_incrementally(self, len).map(Cow::Owned)
    }

    /// Read a LEB128 varint as a `u64`.
    ///
    /// # Errors
    ///
    /// - [`SerialError::VarintOverflow`] for an encoding wider than 10
    ///   bytes or one whose value overflows `u64`.
    /// - [`SerialError::UnexpectedEof`] for a truncated encoding.
    /// - [`SerialError::NonCanonical`] for a multi-byte encoding with a
    ///   redundant zero final byte, when [`Config::canonical`] is on.
    #[inline]
    fn read_varint_u64(&mut self) -> Result<u64> {
        if self.config().canonical {
            read_varint_u64_with::<Self, true>(self)
        } else {
            read_varint_u64_with::<Self, false>(self)
        }
    }

    /// Read a LEB128 varint as a `u128`.
    ///
    /// # Errors
    ///
    /// See [`Decode::read_varint_u64`] (the width limit is 19 bytes).
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
                if consumed > 1 && byte == 0 {
                    return overlong_varint(self, result);
                }
                return Ok(result);
            }
            shift += 7;
        }
        Err(SerialError::VarintOverflow)
    }

    /// Read a length-prefixed byte run into a fresh `Vec<u8>`.
    ///
    /// The length is read as a varint and validated against
    /// [`Decode::max_alloc`]; the bytes are then read with
    /// [`Decode::read_byte_cow`], which never allocates ahead of the data
    /// that actually exists.
    ///
    /// # Errors
    ///
    /// - [`SerialError::InvalidLength`] if the prefix exceeds `max_alloc`.
    /// - [`SerialError::UnexpectedEof`] if the source runs out before the
    ///   declared length is satisfied.
    #[inline]
    fn read_length_prefixed(&mut self) -> Result<Vec<u8>> {
        let declared = self.read_varint_u64()?;
        let len = guard_element_count(declared, self)?;
        self.read_byte_cow(len).map(Cow::into_owned)
    }
}

/// LEB128 `u64` decode loop. With `CANONICAL` the redundant-zero-byte check
/// is compiled in; without it the loop is exactly the lenient 1.0 decoder,
/// so lenient decoding pays nothing for canonical mode existing.
#[inline]
pub(crate) fn read_varint_u64_with<D: Decode + ?Sized, const CANONICAL: bool>(
    decoder: &mut D,
) -> Result<u64> {
    // Fast path for single-byte varints (values 0..=127, the
    // overwhelmingly common case for length prefixes and small ints).
    let first = decoder.read_byte()?;
    if first < 0x80 {
        return Ok(u64::from(first));
    }
    let mut result: u64 = u64::from(first & 0x7f);
    let mut shift: u32 = 7;
    for consumed in 2..=varint::MAX_VARINT_LEN_U64 {
        let byte = decoder.read_byte()?;
        // The 10th byte may only set bit 0 — anything else overflows u64.
        if consumed == varint::MAX_VARINT_LEN_U64 && (byte & 0xfe) != 0 {
            return Err(SerialError::VarintOverflow);
        }
        result |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            // A zero final byte after a continuation byte adds nothing:
            // the value has a shorter encoding.
            if CANONICAL && byte == 0 {
                return Err(SerialError::NonCanonical { reason: "varint" });
            }
            return Ok(result);
        }
        shift += 7;
    }
    Err(SerialError::VarintOverflow)
}

/// A multi-byte varint ended in `0x00`: valid for lenient decoders, a
/// [`SerialError::NonCanonical`] in canonical mode. Kept out of line so the
/// varint hot loop stays small; canonical encoders never reach it.
#[cold]
#[inline(never)]
fn overlong_varint<D: Decode + ?Sized, T>(decoder: &D, value: T) -> Result<T> {
    if decoder.config().canonical {
        return Err(SerialError::NonCanonical { reason: "varint" });
    }
    Ok(value)
}

/// The default body of [`Decode::read_byte_cow`]: read `len` bytes into a
/// buffer that doubles (starting at [`PREALLOC_BYTES`]) only after the
/// previous chunk has been filled from the source.
///
/// Peak allocation is therefore at most about twice the bytes the source
/// actually delivered, plus one chunk — never the declared `len`.
fn read_bytes_incrementally<D: Decode + ?Sized>(decoder: &mut D, len: usize) -> Result<Vec<u8>> {
    if let Some(remaining) = decoder.remaining_hint() {
        if len > remaining {
            return Err(SerialError::UnexpectedEof {
                needed: len,
                remaining,
            });
        }
    }
    // The first chunk comes from a zeroed allocation (cheap for the common
    // short value); later chunks double only after the previous one filled.
    let mut filled = len.min(PREALLOC_BYTES);
    let mut out: Vec<u8> = alloc::vec![0u8; filled];
    decoder.read_into(&mut out)?;
    while filled < len {
        let target = len.min(filled.saturating_mul(2).max(PREALLOC_BYTES));
        out.resize(target, 0);
        decoder.read_into(&mut out[filled..target])?;
        filled = target;
    }
    Ok(out)
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

    /// Construct an encoder with an output buffer pre-allocated to
    /// `capacity` bytes.
    ///
    /// Choose this over [`Encoder::new`] when the encoded size is roughly
    /// known: a single `Vec::with_capacity` up front avoids the four to
    /// eight grow-and-copy reallocations that a zero-capacity `Vec`
    /// performs while doubling to the final size.
    ///
    /// `capacity` is a hint — the encoder still grows the buffer if the
    /// encoded value exceeds it. Setting it slightly too high is harmless;
    /// setting it slightly too low costs at most one growth.
    ///
    /// The Tier-1 [`crate::encode`] free function uses a small default
    /// capacity internally so most one-shot encodes never grow at all.
    ///
    /// # Examples
    ///
    /// ```
    /// let enc = pack_io::Encoder::with_capacity(256);
    /// assert!(enc.as_bytes().is_empty());
    /// ```
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            out: Vec::with_capacity(capacity),
        }
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

    /// Borrow the encoded bytes accumulated so far, without consuming the
    /// encoder.
    ///
    /// Prefer this over [`Encoder::into_inner`] when the caller wants to
    /// send / hash / inspect the bytes but keep writing more values.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::Encoder;
    ///
    /// let mut enc = Encoder::new();
    /// enc.write(&1_u64).unwrap();
    /// let snapshot = enc.as_bytes().to_vec();
    /// enc.write(&2_u64).unwrap();
    /// assert_eq!(snapshot, &[0x01]);
    /// assert_eq!(enc.as_bytes(), &[0x01, 0x02]);
    /// ```
    #[inline]
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.out
    }

    /// Consume the encoder and return its underlying buffer.
    ///
    /// The returned `Vec<u8>` is the exact bytes accumulated by every
    /// preceding `write` call.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::Encoder;
    ///
    /// let mut enc = Encoder::new();
    /// enc.write(&7_u64).unwrap();
    /// let bytes: Vec<u8> = enc.into_inner();
    /// assert_eq!(bytes, &[0x07]);
    /// ```
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> Vec<u8> {
        self.out
    }

    /// Swap the encoder's buffer with a fresh empty one, returning the bytes
    /// written so far. Useful for "encode then send" loops that want to
    /// re-use the encoder across many messages.
    ///
    /// After `take`, the encoder is empty and ready to encode the next
    /// message. The returned buffer is the previous contents.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::Encoder;
    ///
    /// let mut enc = Encoder::new();
    /// enc.write(&1_u64).unwrap();
    /// let first = enc.take();
    /// enc.write(&2_u64).unwrap();
    /// let second = enc.take();
    /// assert_eq!(first, &[0x01]);
    /// assert_eq!(second, &[0x02]);
    /// assert!(enc.as_bytes().is_empty());
    /// ```
    #[must_use]
    pub fn take(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.out)
    }

    /// Encode `value`, appending its bytes to the internal buffer.
    ///
    /// This is the [`Serialize`]-aware sibling of [`Encode::write_bytes`].
    /// Prefer it for typed values; reserve `write_bytes` for raw byte
    /// passthrough.
    ///
    /// # Errors
    ///
    /// Propagates any error returned by the type's [`Serialize`]
    /// implementation. The built-in primitive and collection impls in this
    /// crate never error on an in-memory encoder.
    ///
    /// # Examples
    ///
    /// Writing several values in sequence:
    ///
    /// ```
    /// use pack_io::Encoder;
    ///
    /// let mut enc = Encoder::new();
    /// enc.write(&7_u64).unwrap();
    /// enc.write(&"hello").unwrap();
    /// enc.write(&vec![1u8, 2, 3]).unwrap();
    /// assert!(!enc.as_bytes().is_empty());
    /// ```
    ///
    /// Encoding a tuple in one call (anything that implements `Serialize`
    /// works):
    ///
    /// ```
    /// use pack_io::Encoder;
    ///
    /// let mut enc = Encoder::new();
    /// enc.write(&(7_u64, "hello", true)).unwrap();
    /// assert!(!enc.as_bytes().is_empty());
    /// ```
    #[inline]
    pub fn write<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        value.serialize(self)
    }
}

impl Encode for Encoder {
    #[inline(always)]
    fn write_byte(&mut self, byte: u8) -> Result<()> {
        self.out.push(byte);
        Ok(())
    }

    #[inline(always)]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.out.extend_from_slice(bytes);
        Ok(())
    }

    #[inline(always)]
    fn reserve(&mut self, additional: usize) {
        self.out.reserve(additional);
    }

    /// Override of [`Encode::write_varint_u64`] specialised for the in-memory
    /// encoder. Pushes each varint byte directly onto the underlying `Vec`,
    /// reserving the full max-width up front so the loop never re-checks
    /// capacity. Avoids the stack-buffer + `extend_from_slice` round-trip
    /// the default impl would perform.
    #[inline]
    fn write_varint_u64(&mut self, value: u64) -> Result<()> {
        if value < 0x80 {
            self.out.push(value as u8);
            return Ok(());
        }
        // Up to 10 bytes for u64. Reserve once, then push without further
        // capacity checks.
        self.out.reserve(varint::MAX_VARINT_LEN_U64);
        let mut n = value;
        while n >= 0x80 {
            self.out.push((n as u8) | 0x80);
            n >>= 7;
        }
        self.out.push(n as u8);
        Ok(())
    }

    /// Same specialisation as [`Encode::write_varint_u64`], widened to 128
    /// bits.
    #[inline]
    fn write_varint_u128(&mut self, value: u128) -> Result<()> {
        if value < 0x80 {
            self.out.push(value as u8);
            return Ok(());
        }
        self.out.reserve(varint::MAX_VARINT_LEN_U128);
        let mut n = value;
        while n >= 0x80 {
            self.out.push((n as u8) | 0x80);
            n >>= 7;
        }
        self.out.push(n as u8);
        Ok(())
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
    depth: usize,
}

impl<'a> Decoder<'a> {
    /// Construct a decoder over `bytes` using the default [`Config`]
    /// (1 GiB `max_alloc`).
    ///
    /// For tighter allocation limits on untrusted input, use
    /// [`Decoder::with_config`] instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::Decoder;
    ///
    /// let bytes = pack_io::encode(&42_u64).unwrap();
    /// let mut dec = Decoder::new(&bytes);
    /// let n: u64 = dec.read().unwrap();
    /// assert_eq!(n, 42);
    /// ```
    #[inline]
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            input: bytes,
            pos: 0,
            config: Config::new(),
            depth: 0,
        }
    }

    /// Construct a decoder with the supplied configuration.
    ///
    /// Use this when the input comes from an untrusted producer and the
    /// caller wants to bound per-value allocations.
    ///
    /// # Errors
    ///
    /// - [`SerialError::InvalidLength`] if `config.max_alloc == 0`.
    /// - [`SerialError::InvalidConfig`] if `config.max_depth == 0`.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::{Config, Decoder};
    ///
    /// let cfg = Config::new().with_max_alloc(16 * 1024); // 16 KiB cap
    /// let bytes = pack_io::encode(&"hello").unwrap();
    /// let mut dec = Decoder::with_config(&bytes, cfg).unwrap();
    /// let s: String = dec.read().unwrap();
    /// assert_eq!(s, "hello");
    /// ```
    pub fn with_config(bytes: &'a [u8], config: Config) -> Result<Self> {
        Ok(Self {
            input: bytes,
            pos: 0,
            config: config.validate()?,
            depth: 0,
        })
    }

    /// Decoder over a sub-slice that inherits `config` and the current
    /// nesting `depth` of the decoder it was carved out of. Used for the
    /// bodies of versioned structs, so a body is decoded under exactly the
    /// limits the caller asked for.
    #[inline]
    pub(crate) fn nested(bytes: &'a [u8], config: Config, depth: usize) -> Self {
        Self {
            input: bytes,
            pos: 0,
            config,
            depth,
        }
    }

    /// The configuration this decoder enforces.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::{Config, Decoder};
    ///
    /// let cfg = Config::new().with_max_depth(8);
    /// let dec = Decoder::with_config(&[], cfg).unwrap();
    /// assert_eq!(dec.config().max_depth, 8);
    /// ```
    #[inline]
    #[must_use]
    pub fn config(&self) -> Config {
        self.config
    }

    /// Bytes consumed so far from the start of the input.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::Decoder;
    ///
    /// let bytes = pack_io::encode(&(1_u8, 2_u8)).unwrap();
    /// let mut dec = Decoder::new(&bytes);
    /// assert_eq!(dec.position(), 0);
    /// let _: u8 = dec.read().unwrap();
    /// assert_eq!(dec.position(), 1);
    /// ```
    #[inline]
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Number of bytes remaining in the input.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::Decoder;
    ///
    /// let bytes = pack_io::encode(&(1_u8, 2_u8, 3_u8)).unwrap();
    /// let mut dec = Decoder::new(&bytes);
    /// assert_eq!(dec.remaining(), 3);
    /// let _: u8 = dec.read().unwrap();
    /// assert_eq!(dec.remaining(), 2);
    /// ```
    #[inline]
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.input.len().saturating_sub(self.pos)
    }

    /// True when there are no more bytes to read.
    ///
    /// Useful as the loop condition for multi-value decode passes that
    /// don't have an explicit count up front.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::{Encoder, Decoder};
    ///
    /// let mut enc = Encoder::new();
    /// enc.write(&1_u64).unwrap();
    /// enc.write(&2_u64).unwrap();
    /// let bytes = enc.into_inner();
    ///
    /// let mut dec = Decoder::new(&bytes);
    /// let mut sum = 0_u64;
    /// while !dec.is_empty() {
    ///     sum += dec.read::<u64>().unwrap();
    /// }
    /// assert_eq!(sum, 3);
    /// ```
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Decode a value of type `T` from the current position, advancing the
    /// cursor by the number of bytes the value consumed.
    ///
    /// # Errors
    ///
    /// Returns any [`SerialError`] surfaced by `T::deserialize`.
    ///
    /// # Examples
    ///
    /// Reading several values in sequence:
    ///
    /// ```
    /// use pack_io::{Encoder, Decoder};
    ///
    /// let mut enc = Encoder::new();
    /// enc.write(&7_u64).unwrap();
    /// enc.write(&"hello").unwrap();
    /// let bytes = enc.into_inner();
    ///
    /// let mut dec = Decoder::new(&bytes);
    /// let n: u64 = dec.read().unwrap();
    /// let s: String = dec.read().unwrap();
    /// assert_eq!((n, s.as_str()), (7, "hello"));
    /// ```
    ///
    /// Reading a tuple in a single call:
    ///
    /// ```
    /// use pack_io::Decoder;
    ///
    /// let bytes = pack_io::encode(&(7_u64, true)).unwrap();
    /// let mut dec = Decoder::new(&bytes);
    /// let pair: (u64, bool) = dec.read().unwrap();
    /// assert_eq!(pair, (7, true));
    /// ```
    #[inline]
    pub fn read<T: Deserialize>(&mut self) -> Result<T> {
        T::deserialize(self)
    }

    /// Read a length-prefixed byte run as a **borrowed** slice of the
    /// underlying input — no allocation, no copy.
    ///
    /// The borrowed slice has the same lifetime `'a` as the decoder's
    /// input buffer, which lets caller-side `&'a str` / `&'a [u8]` decode
    /// paths return a borrow directly into that buffer. This is the seam
    /// the zero-copy [`crate::DeserializeView`] surface plugs into for
    /// `&'a str` and `&'a [u8]`.
    ///
    /// # Errors
    ///
    /// - [`SerialError::InvalidLength`] if the prefix exceeds the
    ///   configured `max_alloc`, OR exceeds the remaining input.
    /// - [`SerialError::UnexpectedEof`] is folded into `InvalidLength` for
    ///   this method, since the buffer length is known up front and a
    ///   declared length running off the end is logically a length-prefix
    ///   error, not a streaming EOF.
    #[inline]
    pub fn read_length_prefixed_borrowed(&mut self) -> Result<&'a [u8]> {
        let declared = <Self as Decode>::read_varint_u64(self)?;
        let remaining = self.remaining();
        let max = self.config.max_alloc as u64;
        if declared > max || declared > remaining as u64 {
            return Err(SerialError::InvalidLength {
                declared,
                remaining,
            });
        }
        // `declared <= remaining`, so the conversion is lossless.
        self.read_borrowed(declared as usize)
    }

    /// Read exactly `len` bytes as a **borrowed** slice of the input — no
    /// length prefix, no allocation, no copy.
    ///
    /// Use it for fixed-size fields whose length is known from the type or
    /// from an earlier field.
    ///
    /// # Errors
    ///
    /// [`SerialError::UnexpectedEof`] if fewer than `len` bytes remain; the
    /// cursor does not move.
    ///
    /// # Examples
    ///
    /// ```
    /// use pack_io::Decoder;
    ///
    /// let mut dec = Decoder::new(&[1, 2, 3, 4]);
    /// let head: &[u8] = dec.read_borrowed(3).unwrap();
    /// assert_eq!(head, &[1, 2, 3]);
    /// assert_eq!(dec.remaining(), 1);
    /// ```
    #[inline]
    pub fn read_borrowed(&mut self, len: usize) -> Result<&'a [u8]> {
        let remaining = self.remaining();
        if len > remaining {
            return Err(SerialError::UnexpectedEof {
                needed: len,
                remaining,
            });
        }
        let start = self.pos;
        let end = start + len;
        let input: &'a [u8] = self.input;
        self.pos = end;
        Ok(&input[start..end])
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

    #[inline]
    fn remaining_hint(&self) -> Option<usize> {
        Some(self.remaining())
    }

    #[inline]
    fn config(&self) -> Config {
        self.config
    }

    #[inline]
    fn depth(&self) -> usize {
        self.depth
    }

    #[inline]
    fn enter_nested(&mut self) -> Result<()> {
        if self.depth >= self.config.max_depth {
            return Err(SerialError::DepthLimitExceeded {
                max_depth: self.config.max_depth,
            });
        }
        self.depth += 1;
        Ok(())
    }

    #[inline]
    fn leave_nested(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    #[inline]
    fn consumed(&self) -> Option<&[u8]> {
        self.input.get(..self.pos)
    }

    /// In-memory specialisation: a bounds check and a borrow of the input.
    #[inline]
    fn read_byte_cow(&mut self, len: usize) -> Result<Cow<'_, [u8]>> {
        self.read_borrowed(len).map(Cow::Borrowed)
    }

    /// In-memory specialisation: validates the length against the actual
    /// buffer as well as `max_alloc`, so a truncated input is rejected
    /// without allocating, then copies exactly the declared bytes.
    #[inline]
    fn read_length_prefixed(&mut self) -> Result<Vec<u8>> {
        self.read_length_prefixed_borrowed().map(<[u8]>::to_vec)
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
    // Pre-reserve enough to hold a typical small-to-medium message in a
    // single allocation. A zero-capacity `Vec` doubles 8+ times before
    // hitting 512 bytes, with each doubling memcpy-ing the prior contents
    // — accounting for a large fraction of the encode-time gap vs codecs
    // that pre-size their output buffer. 512 bytes covers most network
    // messages without growth; larger payloads pay at most one or two
    // doublings instead of the eight-plus a fresh `Vec` would.
    let mut enc = Encoder::with_capacity(512);
    value.serialize(&mut enc)?;
    Ok(enc.into_inner())
}

/// Peek the schema version of a payload produced by a `#[pack_io(version = N)]`
/// type without consuming the buffer.
///
/// Reads only the leading varint and returns it as `u32`, leaving the
/// caller free to dispatch decode to the right `T` based on what they find.
/// On a non-versioned payload (no `#[pack_io(version = N)]` on the type)
/// this returns whatever the first varint of the encoding happens to be —
/// callers should only use it on payloads they know are versioned.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "derive")] {
/// use pack_io::{encode, peek_version, Serialize, Deserialize};
///
/// #[derive(Serialize, Deserialize)]
/// #[pack_io(version = 2)]
/// struct Msg { id: u64 }
///
/// let bytes = encode(&Msg { id: 7 }).unwrap();
/// assert_eq!(peek_version(&bytes).unwrap(), 2);
/// # }
/// ```
///
/// # Errors
///
/// - [`SerialError::UnexpectedEof`] if `bytes` is empty or the leading
///   varint is truncated.
/// - [`SerialError::VarintOverflow`] / [`SerialError::IntegerOutOfRange`]
///   if the leading varint does not fit in `u32`.
#[inline]
pub fn peek_version(bytes: &[u8]) -> Result<u32> {
    let mut dec = Decoder::new(bytes);
    let v = dec.read_varint_u64()?;
    u32::try_from(v).map_err(|_| SerialError::IntegerOutOfRange)
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
    finish(&dec, value)
}

/// [`decode`] under an explicit [`Config`]: decode a value of type `T` from
/// `bytes`, requiring the input to be fully consumed.
///
/// Use it to tighten [`Config::max_alloc`] / [`Config::max_depth`] for
/// untrusted input, or to turn on [`Config::canonical`] where the bytes are
/// hashed or signed.
///
/// # Examples
///
/// ```
/// use pack_io::{Config, SerialError, decode_with_config, encode};
///
/// let bytes = encode(&vec![0u8; 1024]).unwrap();
/// let tight = Config::new().with_max_alloc(256);
/// let err = decode_with_config::<Vec<u8>>(&bytes, tight).unwrap_err();
/// assert!(matches!(err, SerialError::InvalidLength { declared: 1024, .. }));
/// ```
///
/// # Errors
///
/// - Any error [`Decoder::with_config`] reports for an invalid `config`.
/// - [`SerialError::TrailingBytes`] when extra bytes follow the value.
/// - Any [`SerialError`] from the type's [`Deserialize`] impl.
#[inline]
pub fn decode_with_config<T: Deserialize>(bytes: &[u8], config: Config) -> Result<T> {
    let mut dec = Decoder::with_config(bytes, config)?;
    let value = T::deserialize(&mut dec)?;
    finish(&dec, value)
}

/// Reject trailing input after a whole-buffer decode.
#[inline]
pub(crate) fn finish<T>(dec: &Decoder<'_>, value: T) -> Result<T> {
    let remaining = dec.remaining();
    if remaining != 0 {
        return Err(SerialError::TrailingBytes { remaining });
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// Size counting
// ---------------------------------------------------------------------------

/// Number of bytes the LEB128 varint encoding of `value` occupies (1–10).
///
/// Handy for hand-written [`Serialize::encoded_len`] overrides, which must
/// account for every length prefix exactly.
///
/// # Examples
///
/// ```
/// assert_eq!(pack_io::varint_len(0), 1);
/// assert_eq!(pack_io::varint_len(127), 1);
/// assert_eq!(pack_io::varint_len(128), 2);
/// assert_eq!(pack_io::varint_len(u64::MAX), 10);
/// assert_eq!(pack_io::varint_len(300), pack_io::encode(&300_u64).unwrap().len());
/// ```
#[inline]
#[must_use]
pub const fn varint_len(value: u64) -> usize {
    // 1 byte per started group of 7 significant bits; zero still takes 1.
    let bits = 64 - (value | 1).leading_zeros() as usize;
    bits.div_ceil(7)
}

/// An [`Encode`] sink that discards bytes and only counts them. Backs the
/// default [`Serialize::encoded_len`].
#[derive(Debug, Default)]
pub struct SizeCounter {
    len: usize,
}

impl SizeCounter {
    /// Bytes counted so far.
    #[inline]
    pub fn count(&self) -> usize {
        self.len
    }
}

impl Encode for SizeCounter {
    #[inline]
    fn write_byte(&mut self, _byte: u8) -> Result<()> {
        self.len = self.len.saturating_add(1);
        Ok(())
    }

    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.len = self.len.saturating_add(bytes.len());
        Ok(())
    }

    #[inline]
    fn write_varint_u64(&mut self, value: u64) -> Result<()> {
        self.len = self.len.saturating_add(varint_len(value));
        Ok(())
    }
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
    fn test_config_builders_and_validation() {
        let cfg = Config::new()
            .with_max_alloc(10)
            .with_max_depth(3)
            .with_canonical(true);
        assert_eq!((cfg.max_alloc, cfg.max_depth, cfg.canonical), (10, 3, true));
        assert_eq!(Config::default(), Config::new());
        assert!(matches!(
            Decoder::with_config(&[], Config::new().with_max_depth(0)),
            Err(SerialError::InvalidConfig { .. })
        ));
    }

    #[test]
    fn test_varint_len_matches_the_encoder_at_every_boundary() {
        for shift in 0..64 {
            for v in [
                1u64 << shift,
                (1u64 << shift) - 1,
                (1u64 << shift).wrapping_add(1),
            ] {
                let mut enc = Encoder::new();
                enc.write_varint_u64(v).unwrap();
                assert_eq!(varint_len(v), enc.as_bytes().len(), "{v}");
            }
        }
        assert_eq!(varint_len(u64::MAX), 10);
    }

    /// A minimal third-party decoder: only the three required methods.
    struct Minimal<'a> {
        bytes: &'a [u8],
        reads: usize,
    }

    impl Decode for Minimal<'_> {
        fn read_byte(&mut self) -> Result<u8> {
            let (first, rest) = self.bytes.split_first().ok_or(SerialError::UnexpectedEof {
                needed: 1,
                remaining: 0,
            })?;
            self.bytes = rest;
            Ok(*first)
        }

        fn read_into(&mut self, out: &mut [u8]) -> Result<()> {
            if out.len() > self.bytes.len() {
                return Err(SerialError::UnexpectedEof {
                    needed: out.len(),
                    remaining: self.bytes.len(),
                });
            }
            self.reads += 1;
            let (head, rest) = self.bytes.split_at(out.len());
            out.copy_from_slice(head);
            self.bytes = rest;
            Ok(())
        }

        fn max_alloc(&self) -> usize {
            1 << 30
        }
    }

    #[test]
    fn test_provided_methods_work_for_a_minimal_decoder() {
        let big = alloc::vec![7u8; PREALLOC_BYTES * 3 + 5];
        let mut encoded = encode(&big).unwrap();
        encoded.extend(encode(&"tail").unwrap());
        let mut dec = Minimal {
            bytes: &encoded,
            reads: 0,
        };
        // The incremental reader fills in chunks (64 KiB, then doubling).
        let back: Vec<u8> = Deserialize::deserialize(&mut dec).unwrap();
        assert_eq!(back, big);
        assert!(dec.reads >= 2, "read in chunks, got {}", dec.reads);
        let tail: alloc::string::String = Deserialize::deserialize(&mut dec).unwrap();
        assert_eq!(tail, "tail");
        // Defaults: no depth tracking, no consumed bytes, default config.
        assert_eq!(dec.depth(), 0);
        assert!(dec.enter_nested().is_ok());
        dec.leave_nested();
        assert!(dec.consumed().is_none());
        assert_eq!(dec.remaining_hint(), None);
        assert_eq!(dec.config().max_alloc, 1 << 30);
    }

    #[test]
    fn test_canonical_mode_rejects_overlong_u128_varints() {
        let strict = Config::new().with_canonical(true);
        let mut dec = Decoder::with_config(&[0x80, 0x00], strict).unwrap();
        assert_eq!(
            dec.read_varint_u128(),
            Err(SerialError::NonCanonical { reason: "varint" })
        );
        let mut dec = Decoder::new(&[0x80, 0x00]);
        assert_eq!(dec.read_varint_u128(), Ok(0));
    }

    #[test]
    fn test_read_borrowed_and_consumed() {
        let mut dec = Decoder::new(&[1, 2, 3]);
        assert_eq!(dec.read_borrowed(2).unwrap(), &[1, 2]);
        assert_eq!(dec.consumed(), Some(&[1u8, 2][..]));
        assert!(matches!(
            dec.read_borrowed(2),
            Err(SerialError::UnexpectedEof {
                needed: 2,
                remaining: 1
            })
        ));
        assert_eq!(dec.position(), 2, "a failed read does not move the cursor");
    }

    #[test]
    fn test_size_counter_counts_without_storing() {
        let mut counter = SizeCounter::default();
        counter.write_bytes(&[0; 10]).unwrap();
        counter.write_byte(1).unwrap();
        counter.write_varint_u64(300).unwrap();
        assert_eq!(counter.count(), 13);
    }

    #[test]
    fn read_into_short_read_is_rejected() {
        let mut dec = Decoder::new(&[0x01, 0x02]);
        let mut buf = [0u8; 4];
        let err = dec.read_into(&mut buf).expect_err("short read");
        assert!(matches!(err, SerialError::UnexpectedEof { .. }));
    }
}
