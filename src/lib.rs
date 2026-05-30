//! # pack-io
//!
//! Compact binary wire format for Rust. Combines speed, schema evolution, and
//! zero-copy deserialization under a single coherent contract.
//!
//! This release (`v0.1.0`) is the scaffolding milestone — the crate compiles,
//! links, and satisfies the workflow gates (formatting, clippy, tests, docs,
//! audit, deny). Codec logic itself lands across the `0.x` series; see
//! [`docs/API.md`](https://github.com/jamesgober/pack-io/blob/main/docs/API.md)
//! for the current public surface and `.dev/ROADMAP.md` for the schedule.
//!
//! ## Reading order
//!
//! - [`README`](https://github.com/jamesgober/pack-io/blob/main/README.md) —
//!   positioning, quick start, and the Tier-1 / Tier-2 / Tier-3 layering.
//! - [`docs/API.md`](https://github.com/jamesgober/pack-io/blob/main/docs/API.md) —
//!   reference for every public item, with examples.
//! - [`CHANGELOG`](https://github.com/jamesgober/pack-io/blob/main/CHANGELOG.md) —
//!   per-release notes; wire-format changes are flagged.
//!
//! ## Invariants (held across every release)
//!
//! - **Round-trip integrity** — `decode(encode(v)) == v` for every supported
//!   type, under any input.
//! - **Determinism** — the same value always produces the same bytes.
//! - **Safe decode** — no panic, no unbounded allocation, no read past input,
//!   on any byte sequence.
//! - **Wire-format stability** — frozen at `1.0`; any `1.x` decoder reads any
//!   `1.x`-or-earlier encoding.
//!
//! ## `no_std`
//!
//! The crate is `no_std`-capable. The default build enables `std`; disable
//! the default feature to build without it:
//!
//! ```toml
//! pack-io = { version = "0.1", default-features = false }
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
#![forbid(unsafe_code)]

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
        // The compile-time constant tracks the `Cargo.toml` version exactly.
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn version_is_pre_one_zero() {
        // The 0.x series has not frozen the wire format yet.
        let major = VERSION.split('.').next().expect("non-empty version string");
        assert_eq!(major, "0");
    }
}
