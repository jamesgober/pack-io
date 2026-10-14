//! # pack-io
//!
//! Compact binary wire format for Rust. Combines speed, schema evolution,
//! and zero-copy deserialization under a single coherent contract.
//!
//! ## At a glance
//!
//! - **Tier 1** — [`encode`] and [`decode`]: one line each direction.
//!   [`decode_with_config`] / [`decode_view_with_config`] take a [`Config`]
//!   for untrusted or hashed input.
//! - **Tier 2** —
//!   - [`Encoder`] / [`Decoder`] for in-memory buffers.
//!   - [`IoEncoder`] / [`IoDecoder`] for `std::io::Write` / `Read` streams
//!     (`std`-gated).
//!   - [`encode_into`] / [`decode_from`] convenience helpers over Read /
//!     Write.
//! - **Tier 3** — implement [`Serialize`] / [`Deserialize`] on your own
//!   types, or derive them (feature `derive`). Both traits are generic over
//!   the [`Encode`] / [`Decode`] behaviour traits, so one impl works through
//!   every encoder / decoder the crate ships.
//!
//! ## Primitive support
//!
//! Integers (`u8` … `u128`, `i8` … `i128`, `usize` / `isize`, `NonZero*`),
//! `bool`, `char`, `f32`, `f64`, `String` / `&str` / `Box<str>`,
//! fixed-size arrays `[T; N]`, tuples of arity 1…12, `Option<T>`,
//! `Result<T, E>`, `Duration`, `PhantomData<T>`, and `()`. `Rc<T>`,
//! `Arc<T>`, `Box<T>` and `Cow<'_, T>` encode transparently as the value they hold.
//!
//! ## Collection support
//!
//! `Vec<T>` / `&[T]` / `Box<[T]>` / `VecDeque<T>`, `BTreeMap<K, V>`,
//! `BTreeSet<T>`, and (with the default `std` feature) `HashMap<K, V>` and
//! `HashSet<T>`. **Maps and sets encode in canonical key-sorted order** so
//! that hashing, signing, or content-addressing the output is safe
//! regardless of insertion order or hash randomisation.
//!
//! ## Stability
//!
//! The wire format has not changed since 1.0: every 2.x decoder reads every
//! encoding a 1.x or 2.x encoder produced for valid data (checked against
//! golden vectors written by 1.0.1), and every 2.x encoder produces the
//! same bytes for the same value. 2.0 is a major release because decoding
//! now enforces limits and the derive rejects misuse 1.x accepted; see the
//! [migration guide](https://github.com/jamesgober/pack-io/blob/main/docs/MIGRATION.md).
//! The normative spec is
//! [`docs/WIRE_FORMAT.md`](https://github.com/jamesgober/pack-io/blob/main/docs/WIRE_FORMAT.md)
//! and the public surface is listed in
//! [`docs/API.md`](https://github.com/jamesgober/pack-io/blob/main/docs/API.md#frozen-public-surface).
//!
//! ## Quick start
//!
//! ```
//! use pack_io::{encode, decode};
//!
//! let bytes = encode(&(7_u64, true, String::from("hello"))).unwrap();
//! let back: (u64, bool, String) = decode(&bytes).unwrap();
//! assert_eq!(back, (7, true, String::from("hello")));
//! ```
//!
//! ## Invariants
//!
//! - **Round-trip integrity** — `decode(encode(v)) == v` for every
//!   supported type.
//! - **Determinism** — the same value always produces the same bytes,
//!   regardless of insertion order, platform, or build flags. With
//!   [`Config::canonical`] on, the reverse also holds: the decoder accepts
//!   only those bytes, so the bytes identify the value. For a versioned
//!   struct that means only frames at the reader's own version (older and
//!   newer frames decode leniently). The guarantee covers types built from
//!   pack-io's impls and derives; a hand-written impl or `with` helper must
//!   itself accept one encoding per value, and serde sets need
//!   `pack_io::serde::sorted_set`.
//! - **Safe decode** — on any byte sequence: no panic, no read past the
//!   input, no allocation larger than [`Config::max_alloc`] for one value,
//!   no length-prefixed allocation larger than the input can back, at most
//!   `max_alloc` across the whole decode for collection elements that
//!   occupy no input bytes (`Box<()>`, all-`skip` structs, …), and no
//!   nesting deeper than [`Config::max_depth`] (so no stack overflow).
//! - **Wire-format stability** — any 2.x decoder reads any 1.x or 2.x
//!   encoding of valid data.
//!
//! ## Feature flags
//!
//! | Feature | Default | Effect |
//! |---|---|---|
//! | `std` | yes | `std::error::Error`, `HashMap` / `HashSet`, the [`io`] module |
//! | `derive` | no | `#[derive(Serialize, Deserialize, DeserializeView)]` |
//! | `schema` | no | alias for `derive`, kept for compatibility (the schema attributes work with `derive` alone) |
//! | `serde` | no | the [`serde` adapter](https://docs.rs/pack-io/latest/pack_io/serde/index.html): any `serde` type on the pack-io wire |
//! | `bytes` | no | `Serialize` / `Deserialize` for `bytes::Bytes` / `BytesMut`, and `Encode` for `BytesMut` |
//!
//! ## `no_std`
//!
//! `pack-io` is `no_std`-capable. The default build enables `std` for the
//! [`std::error::Error`] impl, `HashMap` / `HashSet` integration, and the
//! [`io`] module. Disable the default feature to compile against `core` +
//! `alloc` only; derives (including versioned ones) work there too:
//!
//! ```toml
//! pack-io = { version = "2", default-features = false, features = ["derive"] }
//! ```

// The links above to `std`-only items resolve to the items themselves when
// `std` is on, and to their docs.rs pages when it is off, so the docs build
// cleanly (`-D warnings`) in every feature configuration.
#![cfg_attr(
    feature = "std",
    doc = "[`IoEncoder`]: crate::IoEncoder\n\
           [`IoDecoder`]: crate::IoDecoder\n\
           [`encode_into`]: crate::encode_into\n\
           [`decode_from`]: crate::decode_from\n\
           [`io`]: crate::io\n\
           [`std::error::Error`]: std::error::Error"
)]
#![cfg_attr(
    not(feature = "std"),
    doc = "[`IoEncoder`]: https://docs.rs/pack-io/2/pack_io/struct.IoEncoder.html\n\
           [`IoDecoder`]: https://docs.rs/pack-io/2/pack_io/struct.IoDecoder.html\n\
           [`encode_into`]: https://docs.rs/pack-io/2/pack_io/fn.encode_into.html\n\
           [`decode_from`]: https://docs.rs/pack-io/2/pack_io/fn.decode_from.html\n\
           [`io`]: https://docs.rs/pack-io/2/pack_io/io/index.html\n\
           [`std::error::Error`]: https://doc.rust-lang.org/std/error/trait.Error.html"
)]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![deny(unused_must_use)]
#![deny(unused_results)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::print_stdout)]
#![deny(clippy::print_stderr)]
#![deny(clippy::dbg_macro)]
#![deny(clippy::unreachable)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "bytes")]
mod bytes_impls;
mod codec;
mod error;
mod impls;
#[cfg(feature = "std")]
pub mod io;
#[cfg(feature = "serde")]
pub mod serde;
mod traits;
mod varint;
mod view;

pub use crate::codec::{
    Config, Decode, Decoder, Encode, Encoder, decode, decode_with_config, encode, peek_version,
    varint_len,
};
pub use crate::error::{Result, SerialError};
pub use crate::traits::{Deserialize, Serialize};
pub use crate::view::{DeserializeView, decode_view, decode_view_with_config};

#[cfg(feature = "std")]
pub use crate::io::{IoDecoder, IoEncoder, decode_from, decode_from_with_config, encode_into};

// Re-export the derive macros when the `derive` feature is on. Users write
// `#[derive(pack_io::Serialize, pack_io::Deserialize, pack_io::DeserializeView)]`
// and the proc-macro crate is the implementation detail.
#[cfg(feature = "derive")]
pub use pack_io_derive::{Deserialize, DeserializeView, Serialize};

/// Semantic version of this crate, as declared in `Cargo.toml`.
///
/// # Examples
///
/// ```
/// // VERSION mirrors Cargo.toml exactly, with no parsing.
/// assert_eq!(pack_io::VERSION, env!("CARGO_PKG_VERSION"));
/// ```
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Support code for `pack-io-derive`. Not public API: the items here exist
/// only so that generated code can reach them through a stable path, and
/// they may change in any release. The derive crate is pinned to this exact
/// version of `pack-io`, so the two always move together.
#[doc(hidden)]
pub mod __private {
    use crate::codec::{Config, Decode, Decoder, Encode, Encoder};
    use crate::error::{Result, SerialError};

    pub use crate::codec::SizeCounter;

    /// Sized adapter so any `E: Encode + ?Sized` can be viewed as
    /// `&mut dyn Encode` by [`Tally`].
    pub struct Fwd<'a, E: Encode + ?Sized>(pub &'a mut E);

    impl<E: Encode + ?Sized> core::fmt::Debug for Fwd<'_, E> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("Fwd")
        }
    }

    impl<E: Encode + ?Sized> Encode for Fwd<'_, E> {
        #[inline]
        fn write_byte(&mut self, byte: u8) -> Result<()> {
            self.0.write_byte(byte)
        }

        #[inline]
        fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
            self.0.write_bytes(bytes)
        }

        #[inline]
        fn reserve(&mut self, additional: usize) {
            self.0.reserve(additional);
        }

        #[inline]
        fn write_varint_u64(&mut self, value: u64) -> Result<()> {
            self.0.write_varint_u64(value)
        }

        #[inline]
        fn write_varint_u128(&mut self, value: u128) -> Result<()> {
            self.0.write_varint_u128(value)
        }
    }

    /// Pass-through encoder that forwards every byte to `inner` and counts
    /// them. Debug builds of derived versioned encodes write the body
    /// through it, so the body length is checked in the same single pass
    /// that writes it (a separate counting pass would re-encode every
    /// nested versioned body once per level of nesting). The inner encoder
    /// is type-erased so recursive types instantiate one `Tally` type, not
    /// one per nesting level.
    pub struct Tally<'a> {
        inner: &'a mut dyn Encode,
        written: usize,
    }

    impl<'a> Tally<'a> {
        /// Wrap `inner`, starting the count at zero.
        #[inline]
        pub fn new(inner: &'a mut dyn Encode) -> Self {
            Self { inner, written: 0 }
        }

        /// Bytes forwarded to the inner encoder so far.
        #[inline]
        pub fn written(&self) -> usize {
            self.written
        }
    }

    impl core::fmt::Debug for Tally<'_> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("Tally")
                .field("written", &self.written)
                .finish_non_exhaustive()
        }
    }

    impl Encode for Tally<'_> {
        #[inline]
        fn write_byte(&mut self, byte: u8) -> Result<()> {
            self.inner.write_byte(byte)?;
            self.written = self.written.saturating_add(1);
            Ok(())
        }

        #[inline]
        fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
            self.inner.write_bytes(bytes)?;
            self.written = self.written.saturating_add(bytes.len());
            Ok(())
        }

        #[inline]
        fn reserve(&mut self, additional: usize) {
            self.inner.reserve(additional);
        }

        #[inline]
        fn write_varint_u64(&mut self, value: u64) -> Result<()> {
            self.inner.write_varint_u64(value)?;
            self.written = self.written.saturating_add(crate::varint_len(value));
            Ok(())
        }
    }

    /// Read and validate the leading `version` varint of a versioned
    /// struct whose type declares `known`: it must fit in `u32` and must
    /// not be the reserved `0`. In canonical mode it must also equal
    /// `known`, because only a frame at the reader's own version has a
    /// single encoding the reader can verify (an older frame is a second
    /// encoding of the defaulted value; a newer one carries fields the
    /// reader cannot check).
    #[inline]
    pub fn read_version<D: Decode + ?Sized>(decoder: &mut D, known: u32) -> Result<u32> {
        let raw = decoder.read_varint_u64()?;
        if raw == 0 {
            return Err(SerialError::InvalidVersion { version: 0 });
        }
        let version = u32::try_from(raw).map_err(|_| SerialError::IntegerOutOfRange)?;
        if version != known && decoder.config().canonical {
            return Err(SerialError::NonCanonical {
                reason: "versioned struct version",
            });
        }
        Ok(version)
    }

    /// Read and validate the `body_len` varint of a versioned struct
    /// against `max_alloc` and, when known, the remaining input.
    #[inline]
    pub fn read_body_len<D: Decode + ?Sized>(decoder: &mut D) -> Result<usize> {
        let declared = decoder.read_varint_u64()?;
        let remaining = decoder.remaining_hint();
        let over_input = remaining.is_some_and(|r| declared > r as u64);
        if declared > decoder.max_alloc() as u64 || over_input {
            return Err(SerialError::InvalidLength {
                declared,
                remaining: remaining.unwrap_or(0),
            });
        }
        usize::try_from(declared).map_err(|_| SerialError::IntegerOutOfRange)
    }

    /// A decoder over a versioned struct body that enforces the caller's
    /// `config`, continues its nesting `depth`, and starts from its running
    /// `unbacked` total (see [`settle_body`]).
    #[inline]
    pub fn body_decoder(
        bytes: &[u8],
        config: Config,
        depth: usize,
        unbacked: usize,
    ) -> Decoder<'_> {
        Decoder::nested(bytes, config, depth, unbacked)
    }

    /// Finish a versioned body and return its final `unbacked` total.
    /// Unread bytes are expected when the payload is newer than the type
    /// (`wire_version > known_version`: fields this reader does not know)
    /// and tolerated in lenient mode. Canonical mode rejects them in every
    /// case ([`read_version`] has already required `wire == known` there).
    #[inline]
    pub fn finish_body(body: &Decoder<'_>, wire_version: u32, known_version: u32) -> Result<usize> {
        let _ = (wire_version, known_version);
        if !body.is_empty() && body.config().canonical {
            return Err(SerialError::NonCanonical {
                reason: "versioned body",
            });
        }
        Ok(Decode::unbacked(body))
    }

    /// Carry the memory a versioned body charged for zero-input collection
    /// elements (`after - before`) over to the decoder the body was read
    /// from, so the `max_alloc` budget spans the whole decode.
    #[inline]
    pub fn settle_body<D: Decode + ?Sized>(
        decoder: &mut D,
        before: usize,
        after: usize,
    ) -> Result<()> {
        if after > before {
            decoder.charge_unbacked(after - before)?;
        }
        Ok(())
    }

    /// In canonical mode, check that the retired slot that started at
    /// `start` in `body` holds exactly the bytes `write_default` produces
    /// (the encoding of the field's default, which is all a conforming
    /// writer puts there). Does nothing in lenient mode.
    #[inline]
    pub fn check_retired<F>(body: &Decoder<'_>, start: usize, write_default: F) -> Result<()>
    where
        F: FnOnce(&mut Encoder) -> Result<()>,
    {
        if !body.config().canonical {
            return Ok(());
        }
        let slot = body
            .consumed()
            .and_then(|consumed| consumed.get(start..))
            .unwrap_or_default();
        let mut expected = Encoder::with_capacity(slot.len());
        write_default(&mut expected)?;
        if expected.as_bytes() != slot {
            return Err(SerialError::NonCanonical {
                reason: "retired field",
            });
        }
        Ok(())
    }

    /// Debug-build check, emitted by the derive after writing a versioned
    /// struct's body, that the body length it wrote up front (from
    /// `encoded_len`) matches the bytes the fields actually produced.
    #[inline]
    pub fn check_body_len(declared: usize, written: usize) -> Result<()> {
        if declared != written {
            return Err(SerialError::custom(
                "pack-io: a field's `encoded_len` disagrees with its `serialize`; the versioned body \
                 length would be wrong",
            ));
        }
        Ok(())
    }

    /// Count the bytes `write` produces, for `#[pack_io(with = "...")]`
    /// fields whose helper module has no `encoded_len`.
    #[inline]
    pub fn count_with<F>(write: F) -> Result<usize>
    where
        F: FnOnce(&mut SizeCounter) -> Result<()>,
    {
        let mut counter = SizeCounter::default();
        write(&mut counter)?;
        Ok(counter.count())
    }

    /// `Err(UnknownVariant)` for an enum tag no variant claims.
    #[inline]
    pub fn unknown_variant<T>(kind: &'static str, index: u64) -> Result<T> {
        Err(SerialError::UnknownVariant { kind, index })
    }
}

/// Runs the code in `docs/MIGRATION.md` as doctests, so the upgrade guide
/// cannot drift from the API it describes.
#[cfg(all(doctest, feature = "derive", feature = "std"))]
#[doc = include_str!("../docs/MIGRATION.md")]
pub struct MigrationGuideDoctests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_cargo_manifest() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    // `alloc::string::String` rather than the prelude `String`, so this
    // test compiles under `cargo test --no-default-features` (no `std`).
    #[test]
    fn tier_one_encode_decode_round_trips_a_tuple() {
        use alloc::string::String;
        let bytes = encode(&(1_u64, true, String::from("hello"))).expect("encode");
        let back: (u64, bool, String) = decode(&bytes).expect("decode");
        assert_eq!(back, (1, true, String::from("hello")));
    }
}
