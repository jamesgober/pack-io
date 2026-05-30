//! # pack-io
//!
//! Compact binary wire format for Rust. Combines speed, schema evolution, and
//! zero-copy deserialization under a single coherent contract.
//!
//! ## At a glance (v0.2.0)
//!
//! - **Tier 1** — [`encode`] and [`decode`]: one line each direction.
//! - **Tier 2** — [`Encoder`] and [`Decoder`]: re-usable buffers, multi-value
//!   streams, validated configuration ([`Config`]).
//! - **Tier 3** — implement [`Serialize`] and [`Deserialize`] directly on
//!   your own types. The derive macro lands in `0.4`.
//!
//! Primitive support today: integers (`u8` … `u128`, `i8` … `i128`,
//! `usize` / `isize`), `bool`, `f32`, `f64`, `String` / `&str`, `Vec<u8>` /
//! `&[u8]`, fixed-size arrays `[T; N]`, tuples of arity 2…12, `Option<T>`,
//! `Result<T, E>`, and `()`.
//!
//! Collections (`Vec<T>`, maps, sets), the zero-copy `View<T>` decode, and
//! the derive macro arrive in later 0.x phases — see
//! [`docs/API.md`](https://github.com/jamesgober/pack-io/blob/main/docs/API.md)
//! for the schedule.
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
//! - **Round-trip integrity** — `decode(encode(v)) == v` for every supported
//!   type, under any input.
//! - **Determinism** — the same value always produces the same bytes.
//! - **Safe decode** — no panic, no unbounded allocation, no read past the
//!   input buffer, on any byte sequence.
//! - **Wire-format stability** — frozen at `1.0`; any `1.x` decoder reads
//!   any `1.x`-or-earlier encoding.
//!
//! ## `no_std`
//!
//! `pack-io` is `no_std`-capable. The default build enables `std` for the
//! [`std::error::Error`] impl on [`SerialError`]; disable it to compile
//! against `core` + `alloc` only:
//!
//! ```toml
//! pack-io = { version = "0.2", default-features = false }
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
mod traits;
mod varint;

pub use crate::codec::{Config, Decoder, Encoder, decode, encode};
pub use crate::error::{Result, SerialError};
pub use crate::traits::{Deserialize, Serialize};

/// Semantic version of this crate, as declared in `Cargo.toml`.
///
/// Useful when downstream code wants to log or report the codec version
/// without parsing the manifest at runtime.
///
/// # Examples
///
/// ```
/// assert!(pack_io::VERSION.starts_with("0."));
/// ```
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_cargo_manifest() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn version_is_pre_one_zero() {
        let major = VERSION.split('.').next().expect("non-empty version");
        assert_eq!(major, "0");
    }

    #[test]
    fn tier_one_encode_decode_round_trips_a_tuple() {
        let bytes = encode(&(1_u64, true, String::from("hello"))).expect("encode");
        let back: (u64, bool, String) = decode(&bytes).expect("decode");
        assert_eq!(back, (1, true, String::from("hello")));
    }
}
