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

## [0.7.0] - 2026-06-04

The **hardening + API freeze** release. v0.7.0 ships zero new public API
and zero wire-format changes — what it adds is the proof that the v0.6
surface is production-ready. Three new infrastructure pieces land
together: an 8-target `cargo-fuzz` continuous harness wired into CI, a
29-test cross-platform byte-equivalence golden vector suite, and a
17-test hand-crafted hostile-input sweep covering recursion bombs,
length prefixes at `u64::MAX`, varint corner cases, and decode_view
attack vectors. Plus the formal public-API freeze recorded as the
authoritative v1.0 contract in
[`docs/API.md`](./docs/API.md#frozen-public-surface).

### Added

- **`fuzz/` cargo-fuzz crate** (workspace-excluded, nightly-only) with
  8 targets, one per public decode entry point:
  - [`decode_string`](./fuzz/fuzz_targets/decode_string.rs) — varint
    length + UTF-8 validation
  - [`decode_vec_u8`](./fuzz/fuzz_targets/decode_vec_u8.rs) — byte-run
    fast path
  - [`decode_tuple`](./fuzz/fuzz_targets/decode_tuple.rs) — mixed
    primitive + length-prefixed shape
  - [`decode_collection`](./fuzz/fuzz_targets/decode_collection.rs) —
    `HashMap<String, Vec<u8>>` count cap + per-entry decode
  - [`decode_view_str`](./fuzz/fuzz_targets/decode_view_str.rs) —
    zero-copy `&str` lifetime / UTF-8 validation
  - [`decode_struct_derive`](./fuzz/fuzz_targets/decode_struct_derive.rs) —
    derive-generated struct deserialiser
  - [`decode_enum_derive`](./fuzz/fuzz_targets/decode_enum_derive.rs) —
    derive-generated enum + variant-index varint
  - [`decode_versioned`](./fuzz/fuzz_targets/decode_versioned.rs) —
    schema-evolution body-length cap
- **CI fuzz job** — every push to `main` runs every fuzz target for
  30 seconds on Ubuntu / nightly. Smoke check catches regressions
  fast; longer continuous fuzzing happens out-of-band (post-1.0
  ossfuzz integration tracked separately).
- [`tests/byte_equivalence.rs`](./tests/byte_equivalence.rs) — **29
  golden-vector tests** asserting known input → known exact bytes.
  Run on every CI matrix cell (Linux / macOS / Windows × stable /
  MSRV) — passing on all six *is* the cross-platform byte-equivalence
  proof. Covers every primitive, all compound types (Option, Result,
  tuples, arrays, BTreeMap canonical ordering), plus a nested-struct
  end-to-end round-trip with the exact expected byte concatenation.
- [`tests/hostile_inputs.rs`](./tests/hostile_inputs.rs) — **17 tests**
  hand-crafting adversarial decode cases that complement the
  `proptest` random-byte sweep: `varint(u64::MAX)` as a length prefix
  across String / Vec<u8> / HashMap / nested Vec, varint corner cases
  at the 10-byte legal boundary (u64) and 19-byte (u128), recursion
  bombs via deeply-nested Option, `decode_view` paths against the
  same hostile inputs, full-prefix truncation sweep of a nested
  struct, and trailing-garbage rejection on both `decode` and
  `decode_view`.
- [`docs/API.md` § Frozen public surface](./docs/API.md#frozen-public-surface) —
  exhaustive enumeration of every type, trait, free function,
  inherent method, derive macro, schema attribute, `SerialError`
  variant, and feature flag in the v1.0 contract, with the version
  each was frozen at. Any item not on the list is an internal detail
  that may change without a major bump.

### Changed

- README status line updated from "pre-1.0, in active development" to
  "pre-1.0, API frozen as of v0.7.0".
- Roadmap entry for v0.7 marked shipped.

### Wire format

**Unchanged.** Every v0.6 payload decodes identically under v0.7. Spec
version remains `1.2`.

### API status: FROZEN

The public surface listed in [`docs/API.md`](./docs/API.md#frozen-public-surface)
is **frozen** as of this release. Source-breaking changes are deferred
to v2.0. Pre-1.0 minor releases (v0.7.x → v0.9.x) ship bug fixes,
hardening passes, performance work, and strictly *additive* changes
only (new `SerialError` variants under the existing `#[non_exhaustive]`
enum; new derive support for new field types).

### Verification

All gates green on **both stable and MSRV 1.85**:

```bash
cargo fmt --all -- --check
cargo +1.85 fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo +1.85 clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo +1.85 test --all-features
cargo build --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo audit
cargo deny check
cd fuzz && cargo +nightly check          # syntax-checks the fuzz crate
```

Test counts at this tag (stable, `--all-features`): **266 total**, all
passing (was 220 in v0.6, +29 byte_equivalence + +17 hostile_inputs).

---

## [0.6.0] - 2026-06-04

The **optimisation pass**. v0.6.0 ships zero new public API and zero
wire-format changes — just three safe-Rust optimisations that close
every gap vs `bincode` / `postcard` / `rkyv` worth closing before 1.0.
Headlines: **encode/log_record went from 219 ns to 38 ns** (82 %
faster) and now leads bincode; **`Vec<u8>` 4 KiB decode went from
2,271 ns to 68 ns** (33× faster) and is tied with bincode within
measurement noise; **64-byte `String` owning decode beats bincode by
12 %** while still enforcing the `Config::max_alloc` defence bincode
skips. Comparative numbers + methodology + honest per-row analysis are
committed at [`docs/PERFORMANCE.md`](./docs/PERFORMANCE.md),
reproducible via `cargo bench --bench comparative --features derive`.

After v0.6 the only remaining benchmark loss is `decode_view` vs
rkyv's archived access (~3×) — a fundamental design difference (rkyv
reads a raw memory layout; pack-io walks varints by the wire-format
spec) that pack-io declines to close on purpose.

### Added

- [`Serialize::serialize_slice`](./src/traits.rs) and
  [`Deserialize::deserialize_many`](./src/traits.rs) — new trait
  methods with default implementations that preserve v0.5 behaviour.
  Types whose batch read / write can be done in a single bulk
  operation override them; the `u8` impl writes / reads via a single
  `extend_from_slice` / `read_into` instead of a per-byte loop. The
  generic `[T]::serialize` and `Vec<T>::deserialize` impls dispatch
  through these methods, so `Vec<u8>` and `&[u8]` payloads take the
  memcpy fast path automatically.
- [`benches/comparative.rs`](./benches/comparative.rs) — comparative
  benchmark suite against `bincode`, `postcard`, `rkyv`. Covers
  encode + decode of a borrow-heavy log record (struct with
  `u64 + level + String + Vec<String> + Vec<u8>`), `u64` round-trips,
  64-byte `String` round-trips (owning + view), and 4 KiB `Vec<u8>`
  decode. New `bincode 2` / `postcard 1` / `rkyv 0.8` / `serde 1` dev
  dependencies justify themselves as benchmark fixtures — they never
  enter the published crate.
- [`docs/PERFORMANCE.md`](./docs/PERFORMANCE.md) — methodology, all
  comparative numbers, and an honest per-row analysis of wins and
  losses. Linked from the README.

### Changed (performance, no behaviour change)

- `Vec<u8>` and `&[u8]` decode is now a single memcpy via the new
  trait-extension fast path. Was 30× slower than bincode in v0.5; now
  tied within measurement noise.
- **Tier-1 [`encode`](./src/codec.rs) pre-reserves 512 bytes of output
  capacity** instead of starting at zero. Most messages fit without
  growing the `Vec`; larger payloads pay at most one or two doublings
  instead of the eight-plus a fresh `Vec` would. This single change
  cuts `encode/log_record` from 134 ns to 39 ns.
- **In-memory [`Encoder`](./src/codec.rs) overrides
  `write_varint_u64` / `write_varint_u128`** to push varint bytes
  directly to the underlying `Vec` after a single capacity reserve,
  avoiding the stack-buffer + `extend_from_slice` round-trip the
  default trait impl performs.
- `#[inline(always)]` on `Encoder::write_byte` / `write_bytes` /
  `reserve` so trait dispatch through the generic `E: Encode + ?Sized`
  parameter consistently inlines after monomorphization.
- `Encode::write_varint_u64` and `Decode::read_varint_u64` short-circuit
  the single-byte case (values < 128, the overwhelmingly common case
  for length prefixes and small ints) — skips the multi-byte path and
  the loop overhead respectively.
- New [`Encoder::with_capacity`](./src/codec.rs) constructor for
  callers who want to pre-size the output buffer explicitly.
- README and roadmap entry for v0.6 updated with the comparative
  numbers; "Speed ✓" claim is now backed by data instead of vibes.

### Wire format

- **Unchanged.** Every v0.5 payload decodes identically under v0.6.
  Spec version remains `1.2`.

### Performance summary

Reproduce on your hardware with
`cargo bench --bench comparative --features derive`. Numbers below are
Criterion medians, Windows x86_64, Rust stable release build, project
release profile (`opt-level = 3, lto = "fat", codegen-units = 1`).

**Wins (pack-io is the fastest):**

| Workload | pack-io | nearest competitor |
|---|---:|---|
| `encode/log_record` (owned struct) | **38 ns** | bincode 40 ns |
| 64-byte `String` owning round-trip | **46 ns** | bincode 52 ns |
| Zero-copy view of 64-byte `&str` | **5.1 ns** | uncontested |

**Ties (within measurement noise):**

| Workload | pack-io | nearest competitor |
|---|---:|---|
| `Vec<u8>` 4 KiB decode | 68 ns | bincode 64 ns |
| `u64` round-trip | 22 ns | bincode 21 ns |
| Owned struct decode | 173 ns | bincode 165 ns / rkyv 153 ns |

**Remaining loss (intentional, documented in [`docs/PERFORMANCE.md`](./docs/PERFORMANCE.md)):**

| Workload | pack-io | winner | Reason |
|---|---:|---|---|
| View vs rkyv archived | 35 ns | rkyv 12 ns (~3× us) | rkyv archive is raw memory layout; pack-io walks varints by spec — fundamental design choice that keeps the wire format implementable from one page. |

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

[Unreleased]: https://github.com/jamesgober/pack-io/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/jamesgober/pack-io/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/jamesgober/pack-io/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/jamesgober/pack-io/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/jamesgober/pack-io/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/jamesgober/pack-io/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jamesgober/pack-io/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/pack-io/releases/tag/v0.1.0
