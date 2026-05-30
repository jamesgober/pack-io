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

[Unreleased]: https://github.com/jamesgober/pack-io/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jamesgober/pack-io/releases/tag/v0.1.0
