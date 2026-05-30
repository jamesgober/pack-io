<h1 align="center">
    <img width="90px" height="auto" src="https://raw.githubusercontent.com/jamesgober/jamesgober/main/media/icons/hexagon-3.svg" alt="Triple Hexagon">
    <br>
    <b>CHANGELOG</b>
</h1>
<p>
  All notable changes to <code>pack-io</code> will be documented in this file. The format is based on <a href="https://keepachangelog.com/en/1.1.0/">Keep a Changelog</a>,
  and this project adheres to <a href="https://semver.org/spec/v2.0.0.html/">Semantic Versioning</a>.
</p>

---

## [Unreleased]

### Added

### Changed

### Fixed

### Security

---

## [0.2.0] - 2026-05-28

The **Foundation** release. The Tier-1 / Tier-2 / Tier-3 codec surface is
live, every supported primitive round-trips, and the safety contract is
locked in by `proptest` harnesses (round-trip, determinism, adversarial
decode). 144 tests pass on stable and MSRV 1.85 across Linux / macOS /
Windows. Codec logic itself is intentionally straightforward — the
optimisation pass lands at `0.6` once the wire format freezes at `0.3`.

The wire format remains unstable across the `0.x` series; the normative
spec and freeze land at `0.3`. See [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md)
(not yet present) when that release ships.

### Added

- Public types:
  - [`pack_io::Serialize`](./src/traits.rs) and [`pack_io::Deserialize`](./src/traits.rs) traits — the seam between a Rust value and its wire-format bytes.
  - [`pack_io::Encoder`](./src/codec.rs) — Tier-2 in-memory encoder. Buffers into an owned `Vec<u8>` with `new`, `into_buffer`, `take`, and `into_inner` for re-use across encodes.
  - [`pack_io::Decoder`](./src/codec.rs) — Tier-2 in-memory decoder. Cursored, bounds-checked, supports `read`, `position`, `remaining`, and `is_empty`.
  - [`pack_io::Config`](./src/codec.rs) — `#[non_exhaustive]` decode-session configuration. `Config::new()` / `Config::with_max_alloc()` are `const fn`. Validated at `Decoder::with_config` construction time.
  - [`pack_io::SerialError`](./src/error.rs) — `#[non_exhaustive]` error enum covering every failure mode (`UnexpectedEof`, `InvalidLength`, `VarintOverflow`, `IntegerOutOfRange`, `InvalidBool`, `InvalidUtf8`, `InvalidTag`, `TrailingBytes`). Implements `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`, and `std::error::Error` under `feature = "std"`.
  - [`pack_io::Result`](./src/error.rs) — `Result<T, SerialError>` alias.
- Tier-1 free functions:
  - [`pack_io::encode`](./src/codec.rs) — encode a value into a freshly allocated `Vec<u8>`.
  - [`pack_io::decode`](./src/codec.rs) — strict decode: rejects trailing bytes with `SerialError::TrailingBytes`.
- `Serialize` / `Deserialize` implementations for every primitive in the
  `0.2` scope: `u8`–`u128`, `i8`–`i128`, `usize`, `isize`, `bool`, `f32`,
  `f64`, `String`, `&str` (encode), `Vec<u8>`, `&[u8]` (encode),
  `[T; N]`, tuples of arity 1 through 12, `Option<T>`, `Result<T, E>`,
  `()`, and `&T` (encode).
- `src/varint.rs` — LEB128 varint encoder / decoder for `u64` and `u128`,
  plus ZigZag mappings for `i64` and `i128`. Decoders reject overlong
  encodings, truncated input, and 10th-byte / 19th-byte overflows. Module
  is fully `pub(crate)`.
- Property-based test suite:
  - [`tests/roundtrip.rs`](./tests/roundtrip.rs) — 25 `proptest` properties asserting `decode(encode(v)) == v` for every primitive (floats compared via `to_bits()`).
  - [`tests/determinism.rs`](./tests/determinism.rs) — 21 `proptest` properties asserting that encoding the same value twice produces identical bytes.
  - [`tests/adversarial.rs`](./tests/adversarial.rs) — 20 tests covering random-bytes safety on every public decode entry point, truncation behaviour, and hostile length-prefix rejection. The decoder never panics, never reads past the input, and refuses allocations above `Config::max_alloc`.
- Examples:
  - [`examples/basic_roundtrip.rs`](./examples/basic_roundtrip.rs) — Tier-1 encode / decode of a heterogeneous tuple.
  - [`examples/primitive_tour.rs`](./examples/primitive_tour.rs) — one encoded value per primitive type, with byte counts.
  - [`examples/reuse_buffer.rs`](./examples/reuse_buffer.rs) — Tier-2 `Encoder::into_buffer` + multi-value `Decoder` loop, no per-round allocation.
- Documentation:
  - [`docs/API.md`](./docs/API.md) — reference for every public item; constructor / method tables, parameter docs, multiple runnable examples per item, the wire-format table, and per-variant error guidance.
  - [`docs/release/v0.2.0.md`](./docs/release/v0.2.0.md) — release note for the Foundation milestone.
- Crate-level documentation rewritten to lead with the Tier-1 quick start
  and the `no_std` story.

### Changed

- `Cargo.toml` version bumped to `0.2.0`.
- The `Encoder` no longer carries a `Config` field — there is nothing on
  the encode path that needs an allocation cap. `Config` exists solely for
  the decode side, where length-prefix validation depends on it.

### Wire format

This release is the first to ship codec bytes. The encoding is **unstable**
through the `0.x` series; the freeze lands at `0.3`. Notable choices:

- LEB128 varint for multi-byte unsigned integers, ZigZag-then-LEB128 for
  signed — the same shape used by `protobuf`, `postcard`, and `bincode`'s
  varint mode. A third-party implementer can read pack-io integers without
  consulting the source.
- Single-byte fixed encoding for `u8` / `i8` (no varint overhead for the
  common case where a `u8` outside a `Vec<u8>` is encoded standalone).
- IEEE 754 bit pattern, little-endian, for `f32` / `f64`. NaN, ±Inf,
  subnormals, and signed zeros round-trip bit-for-bit.
- Tag bytes for `Option` and `Result` are strict: `0x00` / `0x01` only;
  any other byte is rejected with `SerialError::InvalidTag`.
- Bool is strict: `0x00` / `0x01` only; any other byte is rejected with
  `SerialError::InvalidBool`.

---

## [0.1.0] - 2026-05-28

Initial scaffold and repository bootstrap. No codec logic yet — this release
establishes the structure, tooling, and quality gates the implementation will
be built on. The full CI workflow (formatting, lints, unit + doc tests, doc
build with `-D warnings`, `cargo audit`, `cargo deny`) is green on Linux,
macOS, and Windows, on both stable and MSRV 1.85.

### Added

- `Cargo.toml` with full crate metadata, Rust 2024 edition, MSRV 1.85,
  dual `Apache-2.0 OR MIT` license, `docs.rs` configuration, and a
  perf-tuned release profile.
- Feature flags: `std` (default), `derive`, `schema`, `serde` (interop).
- Dev-dependencies for the test stack: `criterion`, `proptest`, and `loom`
  under `cfg(loom)`.
- `pack_io::VERSION` — compile-time `&'static str` mirroring
  `CARGO_PKG_VERSION` so downstream code can log or assert the codec
  version without parsing the manifest. Documented with three runnable
  examples; covered by two unit tests and one doctest.
- `benches/codec_bench.rs` — placeholder Criterion harness that keeps the
  `[[bench]]` target in `Cargo.toml` building cleanly. Real encode / decode
  benchmarks land alongside the codec in `0.2`.
- `README.md` — overview, positioning vs `bincode` / `rkyv` / `postcard`,
  the Tier-1 / Tier-2 / Tier-3 layering, invariant list, roadmap snapshot,
  and the full pre-merge local checklist.
- `docs/API.md` — reference for `pack_io::VERSION`, the target Tier-1 /
  Tier-2 / Tier-3 surface (marked _planned: vX.Y_), the schema-evolution
  attribute surface, the error enum, and per-feature documentation.
- `docs/release/v0.1.0.md` — release note for the scaffold milestone.
- `REPS.md` compliance baseline at the repository root.
- `.github/workflows/ci.yml` — Linux / macOS / Windows CI matrix on stable
  and MSRV (fmt, clippy `-D warnings`, test, doc `-D warnings`), plus
  loom and security (audit + deny) jobs.
- `deny.toml` — `cargo-deny` license / advisory / source policy.
- `.gitattributes` normalising line endings to LF and keeping
  development-only paths out of `git archive` tarballs.
- `.dev/` AI-editor briefing (`PROMPT.md`, `ROADMAP.md`) — gitignored.

### Changed

- `src/lib.rs` upgraded from the trivial smoke-test stub to a documented
  crate root with the `VERSION` constant, the REPS-mandated lint set
  (`deny(missing_docs)`, `deny(clippy::todo)`, `forbid(unsafe_code)`, …),
  and crate-level rustdoc that orients new readers to the README, the API
  reference, and the per-release wire-format notes.

[Unreleased]: https://github.com/jamesgober/pack-io/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/jamesgober/pack-io/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/pack-io/releases/tag/v0.1.0
