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

## [0.4.0] - 2026-05-28

The **derive + zero-copy** release. v0.4.0 ships the `pack-io-derive`
companion crate behind the `derive` feature flag, so user structs and
enums opt into the codec with `#[derive(Serialize, Deserialize)]`. Plus
the `DeserializeView<'a>` trait + `decode_view` free function + the
matching `#[derive(DeserializeView)]` deliver the zero-copy decode path
that returns `&'a str` / `&'a [u8]` borrowing directly out of the input
buffer. Local Criterion microbenchmarks show **~7×** faster decode on a
representative borrow-heavy record and **~14×** faster on a 64-byte
string. 203 tests pass on stable and MSRV 1.85 across Linux / macOS /
Windows.

The wire format gets an additive extension (enums, §3.7 of
[`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md)) — every payload valid
under the v0.3 freeze remains valid under v0.4.

### Added

- **Workspace layout.** The repository becomes a 2-member workspace with
  the proc-macro companion crate [`pack-io-derive`](./pack-io-derive/)
  alongside the main `pack-io` crate. `pack-io-derive = "=0.4.0"` is an
  exact-pinned, optional, path-based dependency gated on the new
  `derive` feature.
- **Derive macros** (re-exported at `pack_io::{Serialize, Deserialize,
  DeserializeView}` under the `derive` feature):
  - `#[derive(Serialize)]` and `#[derive(Deserialize)]` — implement the
    value traits for any struct (named, tuple, unit) and any enum (any
    variant shape), generic over type parameters.
  - `#[derive(DeserializeView)]` — implements the zero-copy
    `DeserializeView<'a>` trait for any single-lifetime struct.
    Per-field `DeserializeView<'a>` is required of every field type.
- **Zero-copy decode surface** (always available, no feature gate):
  - [`pack_io::DeserializeView`](./src/view.rs) — the borrowed
    counterpart to `Deserialize`. Built-in impls for `&'a str`,
    `&'a [u8]`, every primitive, `Option<T>`, `Result<T, E>`,
    tuples (arity 1–12), `[T; N]`, `Vec<T>`, `BTreeMap`, `BTreeSet`,
    `HashMap` *(std)*, `HashSet` *(std)*.
  - [`pack_io::decode_view`](./src/view.rs) — Tier-1 zero-copy entry
    point. Strict: rejects trailing bytes with `SerialError::TrailingBytes`.
  - [`pack_io::Decoder::read_length_prefixed_borrowed`](./src/codec.rs) —
    new inherent method on the in-memory decoder that returns
    `&'a [u8]` borrowing from the input slice. Powers the `&'a str` /
    `&'a [u8]` view impls.
- **Enum wire format** (additive extension to the spec, [`docs/WIRE_FORMAT.md §3.7`](./docs/WIRE_FORMAT.md#37-enums)):
  `varint(variant_index) ++ fields` where `variant_index` is the variant's
  source-declaration position starting at `0`. Unknown indices on decode
  surface as the new [`SerialError::UnknownVariant { kind, index }`](./src/error.rs)
  variant.
- New examples:
  - [`examples/derive_intro.rs`](./examples/derive_intro.rs) — every
    derive-supported shape (named / tuple / unit struct, generics, enum
    variants).
  - [`examples/view_zero_copy.rs`](./examples/view_zero_copy.rs) — owning
    vs zero-copy decode side-by-side, with a runtime pointer-equality
    check confirming the view borrows directly from the source buffer.
- New tests:
  - [`tests/derive.rs`](./tests/derive.rs) — 14 tests covering every
    derive-supported shape, generic structs, enum unit / tuple / named
    variants, `UnknownVariant` rejection, and "derived bytes match
    hand-rolled bytes" determinism.
  - Plus 9 new in-source unit tests + 2 new doctests inside the `view`
    module, and the existing test suites all continue to pass against
    the refactored Decoder.
- Real Criterion benchmark in [`benches/codec_bench.rs`](./benches/codec_bench.rs)
  measuring encode, owning decode, and view decode of a representative
  borrow-heavy record (`u64 + level + String + Vec<String> + Vec<u8>`).
  The placeholder bench from v0.1 is replaced. Captured numbers
  documented in the README.

### Changed

- [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md) bumped to spec version
  `1.1`. Additive only: §3.7 (enums) and the `UnknownVariant` error
  category in §6. Every encoding produced by `1.0` decoders remains
  valid under `1.1`.
- [`docs/API.md`](./docs/API.md) restructured around the new derive +
  zero-copy surface, with a dedicated "Zero-copy decode" section and the
  performance numbers cited inline.

### Wire format

- **New: enum encoding** — `varint(variant_index)` followed by the
  variant's fields in source declaration order, concatenated. See
  [`WIRE_FORMAT.md §3.7`](./docs/WIRE_FORMAT.md#37-enums) for the
  normative spec.
- **Compatibility:** every payload valid under the `1.0` spec (the v0.3
  freeze) remains valid under the `1.1` spec. The enum encoding is a
  new producer / consumer capability — payloads that did not encode an
  enum under `1.0` see no change.
- **Migration note for enum producers:** variant indices are
  source-declaration order. Inserting a variant in the middle of an
  enum declaration shifts the indices of every later variant — a
  per-enum wire-format-breaking change. **Append new variants to the
  end** of the declaration to preserve compatibility.

---

## [0.3.0] - 2026-05-28

The **wire-format freeze** release. v0.3.0 ships the normative byte-level
spec in [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md), the full standard
collection surface (`Vec<T>`, `HashMap`, `HashSet`, `BTreeMap`, `BTreeSet`),
and the streaming codec pair (`IoEncoder<W>`, `IoDecoder<R>`) that runs
through any `std::io::Write` / `Read`. 177 tests pass on stable and MSRV
1.85 across Linux / macOS / Windows.

### Added

- [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md) — normative byte-level
  specification (spec version `1.0`).
- Public traits [`pack_io::Encode`](./src/codec.rs) and
  [`pack_io::Decode`](./src/codec.rs).
- Streaming codec ([`pack_io::IoEncoder`](./src/io.rs),
  [`pack_io::IoDecoder`](./src/io.rs)) plus the
  [`pack_io::encode_into`](./src/io.rs) /
  [`pack_io::decode_from`](./src/io.rs) free functions, all `std`-gated.
- `Serialize` / `Deserialize` impls for `Vec<T>`, `BTreeMap`, `BTreeSet`,
  `HashMap` *(std)*, `HashSet` *(std)*. Hash-based collections encode in
  canonical key-sorted order.
- [`pack_io::SerialError::Io { kind, message }`](./src/error.rs).
- New tests: [`tests/collections.rs`](./tests/collections.rs),
  [`tests/streaming.rs`](./tests/streaming.rs).
- New examples: [`examples/collections_tour.rs`](./examples/collections_tour.rs),
  [`examples/streaming_io.rs`](./examples/streaming_io.rs).

### Changed (breaking)

- `Serialize` / `Deserialize` trait signatures became generic over
  `Encode` / `Decode` (`fn serialize<E: Encode + ?Sized>(...)`).
  Hand-rolled v0.2 impls must update the parameter type.
- `Encoder` no longer carries a `Config` field.
- Specialised `Vec<u8>` / `[u8]` impls replaced by generic `Vec<T>` /
  `[T]` — identical wire format.

### Fixed

- Collection deserializers no longer pre-allocate proportional to the
  declared element count — capped at 4096 entries. Fixes a Windows-CI
  OOM where `HashMap::with_capacity(count)` attempted a multi-GB
  hash-table allocation under hostile inputs.

---

## [0.2.0] - 2026-05-28

The **Foundation** release. Tier-1 / Tier-2 / Tier-3 codec surface live;
every supported primitive round-trips; safety contract locked in by
`proptest` harnesses (round-trip, determinism, adversarial decode).
144 tests pass on stable and MSRV 1.85.

### Added

- Public types `Serialize`, `Deserialize`, `Encoder`, `Decoder`, `Config`,
  `SerialError`, `Result`.
- Tier-1 `encode` / `decode` free functions.
- `Serialize` / `Deserialize` impls for every primitive in `0.2` scope.
- Property-based test suite ([`tests/roundtrip.rs`](./tests/roundtrip.rs),
  [`tests/determinism.rs`](./tests/determinism.rs),
  [`tests/adversarial.rs`](./tests/adversarial.rs)).
- Examples ([`basic_roundtrip`](./examples/basic_roundtrip.rs),
  [`primitive_tour`](./examples/primitive_tour.rs),
  [`reuse_buffer`](./examples/reuse_buffer.rs)).

---

## [0.1.0] - 2026-05-28

Initial scaffold and repository bootstrap. No codec logic yet — this
release establishes the structure, tooling, and quality gates the
implementation will be built on.

### Added

- `Cargo.toml` with full crate metadata, Rust 2024 edition, MSRV 1.85.
- Feature flags: `std` (default), `derive`, `schema`, `serde`.
- `pack_io::VERSION` — compile-time `&'static str`.
- `benches/codec_bench.rs` — placeholder Criterion harness.
- `README.md`, `docs/API.md`, `docs/release/v0.1.0.md`.
- `REPS.md` compliance baseline.
- `.github/workflows/ci.yml`, `deny.toml`, `.gitattributes`.

[Unreleased]: https://github.com/jamesgober/pack-io/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/jamesgober/pack-io/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/jamesgober/pack-io/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jamesgober/pack-io/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/pack-io/releases/tag/v0.1.0
