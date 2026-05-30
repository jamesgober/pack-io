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

## [0.1.0] - 2026-05-29

Initial scaffold and repository bootstrap. No codec logic yet — this release
establishes the structure, tooling, and quality gates the implementation will
be built on.

### Added

- `Cargo.toml` with full crate metadata, Rust 2024 edition, MSRV 1.85,
  dual `Apache-2.0 OR MIT` license, `docs.rs` configuration, and a
  perf-tuned release profile.
- Feature flags: `std` (default), `derive`, `schema`, `serde` (interop).
- Dev-dependencies for the test stack: `criterion`, `proptest`, and `loom`
  under `cfg(loom)`.
- `README.md` — overview, positioning vs `bincode`/`rkyv`/`postcard`,
  Tier-1 quick start, zero-copy and schema evolution examples,
  cross-platform support.
- `docs/API.md` reference skeleton.
- `REPS.md` compliance baseline at the repository root.
- `.github/workflows/ci.yml` — Linux/macOS/Windows CI matrix on stable
  and MSRV (fmt, clippy `-D warnings`, test, doc `-D warnings`), plus
  loom and security (audit + deny) jobs.
- `deny.toml` — cargo-deny license / advisory / source policy.
- `.gitattributes` normalising line endings to LF and keeping
  development-only paths out of `git archive` tarballs.
- `.dev/` AI-editor briefing (`PROMPT.md`, `ROADMAP.md`) — gitignored.

[Unreleased]: https://github.com/jamesgober/pack-io/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jamesgober/pack-io/releases/tag/v0.1.0
