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

- Collection deserializers (`Vec<T>`, `HashMap<K, V>`, `HashSet<T>`) no
  longer pre-allocate proportional to the declared element count. The
  initial capacity is now capped at 4096 entries; the per-element decode
  loop fails fast on `UnexpectedEof` when the source runs out. Fixes the
  Windows-CI OOM regression where a `HashMap<String, u32>` decode with a
  hostile declared count would attempt a multi-GB hash-table allocation
  (each slot ~36 bytes including hash-table overhead). Legitimate large
  collections still decode correctly; the cost is one or two grow-and-
  copy operations during the decode loop. New regression test:
  `declared_count_below_max_alloc_does_not_overcommit_memory`.

### Security

---

## [0.3.0] - 2026-05-28

The **wire-format freeze** release. v0.3.0 ships the normative byte-level
spec in [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md), the full standard
collection surface (`Vec<T>`, `HashMap`, `HashSet`, `BTreeMap`, `BTreeSet`),
and the streaming codec pair (`IoEncoder<W>`, `IoDecoder<R>`) that runs
through any `std::io::Write` / `Read`. 177 tests pass on stable and MSRV
1.85 across Linux / macOS / Windows.

**Hash-based collections encode in canonical key-sorted order** — sorted
lexicographically by their encoded key bytes — so a `HashMap` and a
`BTreeMap` over the same logical data encode to identical bytes regardless
of insertion order or build-flag-dependent hash randomisation. This is the
load-bearing property for hashing, signing, and content-addressing pack-io
payloads.

### Wire-format freeze

Starting at this release the wire format is frozen for the `1.x` line. Any
`1.x` decoder reads any `1.x`-or-earlier encoding. Changes that would
break the format require a `2.x` major version bump.

### Added

- [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md) — normative byte-level
  specification. Written so a reader could implement a compatible codec
  without consulting the source. Covers every primitive, every
  length-prefixed and compound type, the canonical map / set ordering
  rule, the full error taxonomy, and the allocation-cap defence.
- Public traits:
  - [`pack_io::Encode`](./src/codec.rs) — the behavioural seam every
    encoder implements (`write_byte`, `write_bytes`, `reserve`,
    `write_varint_u64`, `write_varint_u128`). Default implementations
    handle the varint cases.
  - [`pack_io::Decode`](./src/codec.rs) — the behavioural seam every
    decoder implements (`read_byte`, `read_into`, `max_alloc`,
    `read_varint_u64`, `read_varint_u128`, `read_length_prefixed`).
    Default implementations handle the varint and length-prefixed cases.
- New public types (Tier 2b, streaming):
  - [`pack_io::IoEncoder<W>`](./src/io.rs) — streaming encoder wrapping
    any `std::io::Write`. Gated on `std` (default-on).
  - [`pack_io::IoDecoder<R>`](./src/io.rs) — streaming decoder wrapping
    any `std::io::Read`. Constructor pair `new` / `with_config`. Gated
    on `std`.
- New public free functions:
  - [`pack_io::encode_into`](./src/io.rs) — single-shot encode straight
    into any `Write`. Gated on `std`.
  - [`pack_io::decode_from`](./src/io.rs) — read all from any `Read`,
    decode the result. Gated on `std`.
- New `Serialize` / `Deserialize` impls for the standard library
  collections:
  - `Vec<T>` and `&[T]` (encode) — generic over `T: Serialize` /
    `Deserialize`. Replaces the `0.2` specialised `Vec<u8>` impls; the
    encoding is identical.
  - `BTreeMap<K, V>` — varint count + entries sorted by encoded-key bytes.
  - `BTreeSet<T>` — varint count + elements sorted by encoded bytes.
  - `HashMap<K, V, S>` *(`std`)* — varint count + entries sorted by
    encoded-key bytes. `K: Serialize` for encode; `K: Deserialize + Hash + Eq`
    and `S: BuildHasher + Default` for decode.
  - `HashSet<T, S>` *(`std`)* — varint count + elements sorted by encoded
    bytes. Same trait bounds as `HashMap`.
- [`pack_io::SerialError::Io`](./src/error.rs) — new variant capturing
  `std::io::ErrorKind` and a stringified message for failures surfaced by
  the streaming codec. Gated on `std`. Preserves `Clone + Eq` on
  `SerialError` by storing the kind and a `String` rather than the
  non-`Clone` `std::io::Error`.
- New integration test suites:
  - [`tests/collections.rs`](./tests/collections.rs) — 18 tests covering
    round-trip for every supported collection, the canonical-encoding
    contract (HashMap vs BTreeMap encode identically; insertion order
    is irrelevant; String-keyed maps remain deterministic), and the
    adversarial defences (hostile element counts, truncation, random-bytes
    panic-freedom).
  - [`tests/streaming.rs`](./tests/streaming.rs) — 11 tests covering
    streaming-vs-in-memory byte equivalence, round-trip through
    `Cursor<Vec<u8>>`, multi-value writer / reader sessions, and the
    `std::io::Error → SerialError::Io` mapping.
- New examples:
  - [`examples/collections_tour.rs`](./examples/collections_tour.rs) —
    round-trips every collection type plus a demonstration of the
    `HashMap` / `BTreeMap` encoding equivalence.
  - [`examples/streaming_io.rs`](./examples/streaming_io.rs) — writes a
    sequence of `Event` records to a tempfile via `IoEncoder`, reads
    them back via `IoDecoder`, plus an `encode_into` / `decode_from`
    cursor round-trip.

### Changed (breaking)

- [`Serialize`](./src/traits.rs) trait signature changed from
  `fn serialize(&self, encoder: &mut Encoder)` to
  `fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E)`. Hand-rolled
  `Serialize` impls from `v0.2` must change the parameter type. Code that
  only calls `pack_io::encode()` / `decode()` is unaffected.
- [`Deserialize`](./src/traits.rs) trait signature changed from
  `fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self>` to
  `fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self>`.
  Same migration as above.
- The `Encoder` no longer carries a `Config` field — `Config` is consumed
  only by [`Decoder`](./src/codec.rs) / [`IoDecoder`](./src/io.rs).
- The specialised `Serialize` / `Deserialize` impls for `[u8]` and
  `Vec<u8>` are replaced by the generic `[T]` / `Vec<T>` impls. The
  resulting wire format is identical; decode performance for very large
  `Vec<u8>` payloads is fractionally lower until the v0.6 optimisation
  pass restores a fast byte-slice path.

### Migration from 0.2.0

Hand-rolled `Serialize` / `Deserialize` impls — change the encoder /
decoder parameter type:

```rust
// v0.2:
impl Serialize for MyType {
    fn serialize(&self, enc: &mut Encoder) -> Result<(), SerialError> { … }
}

// v0.3:
impl Serialize for MyType {
    fn serialize<E: Encode + ?Sized>(&self, enc: &mut E) -> Result<()> { … }
}
```

The body of the impl is unchanged. `pack_io::Result<T>` is now used
throughout in place of `Result<T, SerialError>` (still spelled the same
under the hood).

---

## [0.2.0] - 2026-05-28

The **Foundation** release. The Tier-1 / Tier-2 / Tier-3 codec surface is
live, every supported primitive round-trips, and the safety contract is
locked in by `proptest` harnesses (round-trip, determinism, adversarial
decode). 144 tests pass on stable and MSRV 1.85 across Linux / macOS /
Windows. Codec logic itself is intentionally straightforward — the
optimisation pass lands at `0.6` once the wire format freezes at `0.3`.

### Added

- Public types: [`pack_io::Serialize`](./src/traits.rs),
  [`pack_io::Deserialize`](./src/traits.rs),
  [`pack_io::Encoder`](./src/codec.rs),
  [`pack_io::Decoder`](./src/codec.rs),
  [`pack_io::Config`](./src/codec.rs),
  [`pack_io::SerialError`](./src/error.rs),
  [`pack_io::Result`](./src/error.rs).
- Tier-1 free functions [`pack_io::encode`](./src/codec.rs) and
  [`pack_io::decode`](./src/codec.rs); the latter is strict and rejects
  trailing bytes with `SerialError::TrailingBytes`.
- `Serialize` / `Deserialize` implementations for every primitive in the
  `0.2` scope: `u8`–`u128`, `i8`–`i128`, `usize`, `isize`, `bool`, `f32`,
  `f64`, `String`, `&str` (encode), `Vec<u8>`, `&[u8]` (encode),
  `[T; N]`, tuples of arity 1 through 12, `Option<T>`, `Result<T, E>`,
  `()`, and `&T` (encode).
- `src/varint.rs` — LEB128 varint encoder / decoder for `u64` and `u128`,
  plus ZigZag mappings for `i64` and `i128`.
- Property-based test suite: [`tests/roundtrip.rs`](./tests/roundtrip.rs),
  [`tests/determinism.rs`](./tests/determinism.rs),
  [`tests/adversarial.rs`](./tests/adversarial.rs).
- Examples: [`examples/basic_roundtrip.rs`](./examples/basic_roundtrip.rs),
  [`examples/primitive_tour.rs`](./examples/primitive_tour.rs),
  [`examples/reuse_buffer.rs`](./examples/reuse_buffer.rs).
- Documentation: [`docs/API.md`](./docs/API.md),
  [`docs/release/v0.2.0.md`](./docs/release/v0.2.0.md).

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
  `CARGO_PKG_VERSION`.
- `benches/codec_bench.rs` — placeholder Criterion harness.
- `README.md` — overview, positioning vs `bincode` / `rkyv` / `postcard`,
  the Tier-1 / Tier-2 / Tier-3 layering, invariant list, roadmap snapshot.
- `docs/API.md` — reference skeleton.
- `docs/release/v0.1.0.md` — release note for the scaffold milestone.
- `REPS.md` compliance baseline.
- `.github/workflows/ci.yml` — Linux / macOS / Windows CI matrix on stable
  and MSRV, plus loom and security (audit + deny) jobs.
- `deny.toml`, `.gitattributes`, `.dev/` AI-editor briefing (gitignored).

[Unreleased]: https://github.com/jamesgober/pack-io/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/jamesgober/pack-io/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jamesgober/pack-io/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/pack-io/releases/tag/v0.1.0
