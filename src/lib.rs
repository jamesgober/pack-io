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
//!   only those bytes, so the bytes identify the value.
//! - **Safe decode** — on any byte sequence: no panic, no read past the
//!   input, no allocation larger than [`Config::max_alloc`] for one value
//!   nor larger than the input can back, and no nesting deeper than
//!   [`Config::max_depth`] (so no stack overflow).
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
    use crate::codec::{Config, Decode, Decoder};
    use crate::error::{Result, SerialError};

    pub use crate::codec::SizeCounter;

    /// Read and validate the leading `version` varint of a versioned
    /// struct: it must fit in `u32` and must not be the reserved `0`.
    #[inline]
    pub fn read_version<D: Decode + ?Sized>(decoder: &mut D) -> Result<u32> {
        let raw = decoder.read_varint_u64()?;
        if raw == 0 {
            return Err(SerialError::InvalidVersion { version: 0 });
        }
        u32::try_from(raw).map_err(|_| SerialError::IntegerOutOfRange)
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
    /// `config` and continues its nesting `depth`.
    #[inline]
    pub fn body_decoder(bytes: &[u8], config: Config, depth: usize) -> Decoder<'_> {
        Decoder::nested(bytes, config, depth)
    }

    /// Finish a versioned body. Unread bytes are expected when the payload
    /// is newer than the type (`wire_version > known_version`: fields this
    /// reader does not know). Otherwise they are tolerated in lenient mode
    /// and rejected in canonical mode.
    #[inline]
    pub fn finish_body(body: &Decoder<'_>, wire_version: u32, known_version: u32) -> Result<()> {
        if wire_version <= known_version && !body.is_empty() && body.config().canonical {
            return Err(SerialError::NonCanonical {
                reason: "versioned body",
            });
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
