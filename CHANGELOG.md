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

## [0.5.0] - 2026-06-04

The **schema-evolution + feature freeze** release. v0.5.0 closes the
feature roadmap: the third distinctive pillar — append-only schema
evolution via `#[pack_io(version = N)]` / `since` / `deprecated` — is
live, and from this point forward the codec API and feature surface are
frozen. The v0.6 release is an optimisation pass against the surface that
exists today; no new public API ships before `1.0`.

The wire format gets one additive extension (versioned structs,
[`docs/WIRE_FORMAT.md §3.8`](./docs/WIRE_FORMAT.md#38-versioned-structs))
— every payload valid under v0.4 remains valid under v0.5. Non-versioned
structs are unchanged; the new format is a per-type opt-in via the new
attribute.

This release also rolls in the publish-readiness fixes that did not make
it into the v0.4.0 tag: the missing `pack-io-derive/README.md`, the
intra-doc-link warnings in the derive crate, bundled license files, and
`required-features` declarations on derive-using examples and tests so
`cargo build` succeeds with default features instead of failing on
"cannot find derive macro" errors.

### Added

- **Schema-evolution attributes** (re-exported under the new
  `schema = ["derive"]` feature):
  - `#[pack_io(version = N)]` on a struct — opts the type into the
    versioned wire format. `N` is a positive `u32`; `0` is rejected at
    compile time.
  - `#[pack_io(since = N)]` on a field — marks the field as added in
    version `N` (defaults to `1`). Decoders reading payloads from older
    versions use `Default::default()` for the absent field.
  - `#[pack_io(deprecated = N)]` on a field — marks the field as
    removed in version `N`. Encoders at version `>= N` drop it;
    decoders reading payloads from older versions still read it
    normally. Compile-time validation rejects `deprecated <= since`.
- [`pack_io::peek_version`](./src/codec.rs) — reads only the leading
  `varint(version)` of a versioned payload without consuming the buffer,
  for runtime dispatch on schema version.
- [`docs/WIRE_FORMAT.md §3.8`](./docs/WIRE_FORMAT.md#38-versioned-structs)
  — normative spec for versioned structs:
  `varint(version) ++ varint(body_len) ++ body`, with the cross-version
  decode contract spelled out (`W = K` / `W < K` / `W > K` cases).
- [`tests/schema_evolution.rs`](./tests/schema_evolution.rs) — 15 tests
  covering v1↔v1 self-round-trip, v1↔v2 cross-decode in both
  directions, deprecated-field semantics across three versions of the
  same type, `peek_version` correctness, hostile body-length rejection,
  and three `proptest` invariants over a wide input space.
- [`examples/schema_evolution.rs`](./examples/schema_evolution.rs) —
  cross-version walkthrough showing v1 / v2 byte-level encodings and
  every cross-decode direction succeeding.
- `pack-io-derive/README.md`, `pack-io-derive/LICENSE-APACHE`,
  `pack-io-derive/LICENSE-MIT` — bundled into the published `.crate`
  file so the proc-macro crate is self-contained on crates.io.
- `required-features` declarations on the derive-using bench, examples,
  and integration tests in [`Cargo.toml`](./Cargo.toml) so default-feature
  builds and rust-analyzer skip them cleanly instead of failing on
  "cannot find derive macro" errors.

### Changed

- `schema` Cargo feature is now `schema = ["derive"]` (was empty in
  v0.4) — pulling it in implies the derive feature, since schema
  attributes are read by the derive macro.
- `pack-io-derive` proc-macros now declare `attributes(pack_io)` so
  `#[pack_io(...)]` is recognised as a helper attribute on derived
  types instead of being treated as an unknown attribute.
- Generated `Deserialize` impls for versioned structs only require
  `Default` on fields with `since > 1` or `deprecated.is_some()`.
  Always-live fields use unconditional decode and need no `Default`
  bound — fixing the over-restrictive code path the first cut emitted.
- Doc comments in `pack-io-derive/src/lib.rs` no longer use intra-doc
  links to `pack_io::*` (which don't resolve when the derive crate is
  built standalone for docs.rs). Switched to inline-code references.
- `Cargo.toml` workspace pinning of `pack-io-derive` bumped to `=0.5.0`.

### Wire format

- **New: versioned struct encoding** —
  `varint(version) ++ varint(body_len) ++ body`, where `body` holds the
  concatenated encodings of fields live at `version`. See
  [`docs/WIRE_FORMAT.md §3.8`](./docs/WIRE_FORMAT.md#38-versioned-structs)
  for the normative spec.
- **Spec version: 1.1 → 1.2.** Additive only. Every payload valid under
  `1.1` remains valid under `1.2`; the new format is a per-type opt-in
  through `#[pack_io(version = N)]`. Non-versioned structs encode
  exactly as before.

### Feature freeze

The public API and feature surface are **frozen** as of `v0.5.0`. The
v0.6 release is an optimisation pass — profiling the codec paths,
benchmarking against `bincode` / `postcard` / `rkyv`, and tightening
hot-path code — but ships no new public types, traits, free functions,
or wire-format changes. The next API surface change is `v1.0` itself.

### Migration from 0.4.0

The schema-evolution attributes are purely additive. No v0.4 code
breaks. To opt a type into schema evolution:

1. Add `pack-io = { version = "0.5", features = ["schema"] }` to
   `Cargo.toml`.
2. Tag the type: `#[pack_io(version = 1)]` (start at version 1).
3. When adding a field in a later release, increment the type's
   `version` and tag the new field `#[pack_io(since = N)]`.

---

## [0.4.0] - 2026-05-28

The **derive + zero-copy** release. v0.4.0 ships the `pack-io-derive`
companion crate behind the `derive` feature flag, so user structs and
enums opt into the codec with `#[derive(Serialize, Deserialize)]`. Plus
the `DeserializeView<'a>` trait + `decode_view` free function + the
matching `#[derive(DeserializeView)]` deliver the zero-copy decode path
that returns `&'a str` / `&'a [u8]` borrowing directly out of the input
buffer. Local Criterion microbenchmarks show ~7× faster decode on a
representative borrow-heavy record and ~14× faster on a 64-byte
string. 203 tests pass on stable and MSRV 1.85.

### Added

- Workspace layout with new [`pack-io-derive`](./pack-io-derive/)
  proc-macro crate.
- `#[derive(Serialize)]`, `#[derive(Deserialize)]`,
  `#[derive(DeserializeView)]` re-exported under the `derive` feature.
- [`pack_io::DeserializeView`](./src/view.rs) trait +
  [`pack_io::decode_view`](./src/view.rs) free function +
  [`pack_io::Decoder::read_length_prefixed_borrowed`](./src/codec.rs)
  inherent method (zero-copy seam).
- Enum wire format (`varint(variant_index) ++ fields`,
  [`docs/WIRE_FORMAT.md §3.7`](./docs/WIRE_FORMAT.md#37-enums)).
- [`SerialError::UnknownVariant`](./src/error.rs).
- Examples: [`derive_intro`](./examples/derive_intro.rs),
  [`view_zero_copy`](./examples/view_zero_copy.rs).
- Tests: [`tests/derive.rs`](./tests/derive.rs) — 14 tests covering
  every derive-supported shape.
- Real Criterion benchmark in [`benches/codec_bench.rs`](./benches/codec_bench.rs).

### Changed

- [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md) bumped to spec
  version `1.1` (additive enum encoding + `UnknownVariant`).

---

## [0.3.0] - 2026-05-28

The **wire-format freeze** release. Normative spec
[`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md), standard collection
surface (`Vec<T>`, `HashMap`, `HashSet`, `BTreeMap`, `BTreeSet`),
streaming codec pair (`IoEncoder<W>`, `IoDecoder<R>`). 177 tests pass
on stable and MSRV 1.85.

### Added

- [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md) — spec version `1.0`.
- [`pack_io::Encode`](./src/codec.rs) and
  [`pack_io::Decode`](./src/codec.rs) behaviour traits.
- Streaming codec ([`IoEncoder`](./src/io.rs),
  [`IoDecoder`](./src/io.rs)) + [`encode_into`](./src/io.rs) /
  [`decode_from`](./src/io.rs) helpers.
- Collection impls (hash-based collections sort by encoded-key bytes
  for byte-determinism).
- [`SerialError::Io`](./src/error.rs).
- Examples: [`collections_tour`](./examples/collections_tour.rs),
  [`streaming_io`](./examples/streaming_io.rs).

### Changed (breaking)

- `Serialize` / `Deserialize` became generic over `Encode` / `Decode`.

### Fixed

- Collection deserializers cap initial preallocation at 4096 entries
  (Windows-CI OOM regression fix).

---

## [0.2.0] - 2026-05-28

The **Foundation** release. Tier-1 / Tier-2 / Tier-3 codec surface live;
every supported primitive round-trips; safety contract locked in by
`proptest` harnesses (round-trip, determinism, adversarial decode).
144 tests pass on stable and MSRV 1.85.

### Added

- Public types `Serialize`, `Deserialize`, `Encoder`, `Decoder`,
  `Config`, `SerialError`, `Result`.
- Tier-1 `encode` / `decode` free functions.
- `Serialize` / `Deserialize` impls for every primitive in `0.2` scope.
- Property-based test suite + three foundation examples.

---

## [0.1.0] - 2026-05-28

Initial scaffold and repository bootstrap. No codec logic yet — this
release establishes the structure, tooling, and quality gates the
implementation will be built on.

### Added

- `Cargo.toml`, README, `docs/API.md`, `REPS.md`, CI matrix, deny.toml.
- Feature flags: `std` (default), `derive`, `schema`, `serde`.
- `pack_io::VERSION` compile-time constant.

[Unreleased]: https://github.com/jamesgober/pack-io/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/jamesgober/pack-io/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/jamesgober/pack-io/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/jamesgober/pack-io/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jamesgober/pack-io/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/pack-io/releases/tag/v0.1.0
