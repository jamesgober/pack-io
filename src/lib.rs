//! # pack-io
//!
//! Compact binary wire format for Rust. Combines speed, schema evolution,
//! and zero-copy deserialization under a single coherent contract.
//!
//! ## At a glance
//!
//! - **Tier 1** — [`encode`] and [`decode`]: one line each direction.
//! - **Tier 2** —
//!   - [`Encoder`] / [`Decoder`] for in-memory buffers.
//!   - [`IoEncoder`] / [`IoDecoder`] for `std::io::Write` / `Read` streams
//!     (`std`-gated).
//!   - [`encode_into`] / [`decode_from`] convenience helpers over Read /
//!     Write.
//! - **Tier 3** — implement [`Serialize`] / [`Deserialize`] on your own
//!   types. Both traits are generic over the [`Encode`] / [`Decode`]
//!   behaviour traits, so one impl works through every encoder / decoder
//!   the crate ships.
//!
//! ## Primitive support
//!
//! Integers (`u8` … `u128`, `i8` … `i128`, `usize` / `isize`), `bool`,
//! `f32`, `f64`, `String` / `&str`, fixed-size arrays `[T; N]`, tuples of
//! arity 1…12, `Option<T>`, `Result<T, E>`, and `()`.
//!
//! ## Collection support
//!
//! `Vec<T>` / `&[T]`, `BTreeMap<K, V>`, `BTreeSet<T>`, and (with the
//! default `std` feature) `HashMap<K, V>` and `HashSet<T>`. **Hash-based
//! collections encode in canonical key-sorted order** so that hashing,
//! signing, or content-addressing the output is safe regardless of
//! insertion order or hash randomisation.
//!
//! ## Stability
//!
//! The public API and wire format are frozen for the entire `1.x` line.
//! Any `1.x` decoder reads any `1.x`-or-earlier encoding. See the
//! normative spec at
//! [`docs/WIRE_FORMAT.md`](https://github.com/jamesgober/pack-io/blob/main/docs/WIRE_FORMAT.md)
//! and the frozen public surface at
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
//!   supported type, under any input.
//! - **Determinism** — the same value always produces the same bytes,
//!   regardless of insertion order, platform, or build flags.
//! - **Safe decode** — no panic, no unbounded allocation, no read past
//!   the input, on any byte sequence.
//! - **Wire-format stability** — any `1.x` decoder reads any
//!   `1.x`-or-earlier encoding.
//!
//! ## `no_std`
//!
//! `pack-io` is `no_std`-capable. The default build enables `std` for the
//! [`std::error::Error`] impl, `HashMap` / `HashSet` integration, and the
//! [`io`] module. Disable the default feature to compile against `core` +
//! `alloc` only:
//!
//! ```toml
//! pack-io = { version = "1", default-features = false }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![deny(unused_must_use)]
#![deny(clippy::todo)]
#![deny(clippy::unimplemented)]
#![deny(clippy::print_stdout)]
#![deny(clippy::print_stderr)]
#![deny(clippy::dbg_macro)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod codec;
mod error;
mod impls;
#[cfg(feature = "std")]
pub mod io;
mod traits;
mod varint;
mod view;

pub use crate::codec::{Config, Decode, Decoder, Encode, Encoder, decode, encode, peek_version};
pub use crate::error::{Result, SerialError};
pub use crate::traits::{Deserialize, Serialize};
pub use crate::view::{DeserializeView, decode_view};

#[cfg(feature = "std")]
pub use crate::io::{IoDecoder, IoEncoder, decode_from, encode_into};

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
