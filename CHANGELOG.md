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

## [2.0.0] - 2026-10-13

Major release: hardening, the schema-evolution fix, and the features the
1.x line could not add without breaking someone. Closes every finding of
the 2026-10 library audit. The "no panic / no unbounded allocation on
any input" promise now holds (it did not in 1.x). The schema-evolution
and zero-copy derive paths no longer mis-parse silently, and the
advertised `serde` feature finally exists.

**The wire format is unchanged.** Every byte string a 1.x encoder
produced for valid data is produced identically and decodes to the same
value, checked against golden vectors written by 1.0.1. The version is
2.0 because decoders now reject some input 1.x accepted, the derive
rejects attributes 1.x ignored, and `Box<T>` gained a blanket impl.
Upgrade guide: [`docs/MIGRATION.md`](./docs/MIGRATION.md).

Before publication, an independent re-audit of this release found four
contract gaps (canonical mode on versioned structs, collections of
elements that occupy no input bytes, serde sets, and a misattributed
performance regression) plus smaller items. They are fixed in this
release; the entries below describe the release as published, and
[Re-audit fixes](#re-audit-fixes-pre-publication) lists what changed.

### Breaking Changes

Each item has a before/after snippet in
[`docs/MIGRATION.md`](./docs/MIGRATION.md).

- **Recursion depth limit** (PACK-004). Every derived decoder counts one
  level against `Config::max_depth` (default 128). Deeper input fails
  with `SerialError::DepthLimitExceeded`. 1.x overflowed the stack on
  about 5 KB of hostile input instead. Raise the limit for trusted deep
  data with `Config::with_max_depth`.
- **Version 0 is rejected** (PACK-010) with `SerialError::InvalidVersion`,
  as `docs/WIRE_FORMAT.md` §3.8 has always required. No 1.x encoder
  writes version 0.
- **`decode_from` is bounded** (PACK-011). It read the whole stream into
  memory; it now stops at `max_alloc` bytes (default 1 GiB) and fails
  with `InvalidLength`. Raise it with `decode_from_with_config`.
- **The derive rejects attributes 1.x ignored** (PACK-007). Unknown or
  duplicated `#[pack_io(...)]` attributes anywhere are compile errors
  with spans. That includes enum fields and variants, which 1.x
  ignored, and `since` newer than the struct's `version`, which 1.x
  accepted while silently never encoding the field. Also rejected:
  `removed = N` more than one version ahead of `version` (1.x
  `deprecated = 40` on a version-2 struct), and an explicit Rust
  discriminant that differs from the variant's wire tag (`A = 5` encoded
  as tag 0 in 1.x; add `#[pack_io(index = N)]`).
- **Enum attributes are honoured.** An undocumented `skip`, `with`,
  `default` or `index` on an enum variant or enum field was ignored by
  1.x and is applied by 2.0, so it changes that enum's encoding. Remove
  the attribute to keep the 1.x bytes.
- **`#[pack_io(deprecated = N)]` is replaced** (PACK-005). It is now a
  compile error that names the two replacements:
  - `removed = N` is the exact 1.x encoding, so 1.x payloads keep
    decoding.
  - `retired = N` is new and keeps the field's slot (see Fixed).
- **`Box<T>` has blanket `Serialize`, `Deserialize` and
  `DeserializeView` impls** (PACK-028). A downstream
  `impl pack_io::Serialize for Box<TheirType>` (or `Deserialize`)
  now conflicts with them and must be deleted. The blanket impl encodes
  the same bytes, because `Box` is transparent.
- **Elements that occupy no input bytes are charged against
  `max_alloc`** (re-audit PACK-R01). A collection of `Box<()>`, `Rc<()>`,
  a zero-sized type or a struct whose fields are all `skip` is limited to
  `max_alloc` bytes of such elements (`size_of` each, at least 1) across
  the whole decode. 1.x accepted any count up to `max_alloc` per
  collection, which let a few bytes demand gigabytes.
- **`max_alloc == 0` reports `SerialError::InvalidConfig`** instead of
  `InvalidLength { declared: 0, .. }`, from `Decoder::with_config`,
  `IoDecoder::with_config` and the `*_with_config` functions.

### Security

- **Allocation is bounded by the input** (PACK-001). A 5-byte input could
  make the in-memory decoder allocate and zero 1 GiB for a `Vec<u8>`
  before failing. Declared lengths are now checked against the remaining
  input before anything is allocated. Collection pre-allocation is
  capped by the remaining input and by 64 KiB.
- **The streaming decoder no longer pre-allocates declared lengths**
  (PACK-002). `IoDecoder` grows buffers in bounded chunks (64 KiB, then
  doubling) as bytes arrive. A peer that declares 1 GiB and then stalls
  costs one chunk.
- **Versioned bodies obey the caller's `Config`** (PACK-003). In 1.x the
  body was copied into a fresh buffer of the declared size and decoded
  under a default 1 GiB config, bypassing any tighter `max_alloc`. The
  body is now:
  - checked against the remaining input;
  - borrowed (in memory) or read incrementally (streams);
  - decoded under the caller's config and nesting depth.
- **Elements that occupy no input bytes** (re-audit PACK-R01). Element
  counts were checked only against `max_alloc` per collection, so 4 bytes
  declaring 2^27 `Box<()>` allocated 1 GiB and larger counts aborted on
  out-of-memory. Each element that consumes no input is now charged
  against `max_alloc` for the whole decode, including nested collections,
  versioned bodies, views, `IoDecoder` and the serde adapter. New
  provided `Decode` methods carry the running total.
- **Streams pre-size collections modestly** (re-audit PACK-R10). With no
  known input length, `IoDecoder` reserved 64 KiB per open collection;
  127 nested hostile counts pinned about 10 MB from 635 bytes. The
  reservation is now 4 KiB.
- **Stack exhaustion and unbounded `decode_from`**: see Breaking
  Changes (PACK-004, PACK-011).

### Fixed

- **Retiring a field is safe for old readers** (PACK-005). In 1.x, a
  reader that predated a field's deprecation decoded the next field's
  bytes into that field, without any error.
  - The new `#[pack_io(retired = N)]` keeps the field's slot on the
    wire and writes the field's default into it from version `N`.
  - Every reader stays aligned and sees the default.
  - Readers at version `N` or later discard whatever is in the slot.
  - It costs about one byte (`docs/WIRE_FORMAT.md` §3.8, spec 1.3).
  - `removed = N` keeps the 1.x byte-dropping encoding and its
    reader-first rule.
- **`DeserializeView` honours `#[pack_io(version = N)]`** (PACK-006).
  The view derive ignored the versioned frame and decoded the version
  and body-length prefixes as fields, a silent mis-parse. Views now read
  the same frame as the owning decoder, borrowing the body.
- **Raw identifiers** (PACK-013): `#[pack_io(version = 1)] struct R { r#type: u32 }`
  made the derive panic; it now compiles.
- **Derive hygiene** (PACK-023): enum fields named like generated locals
  (`__encoder`, …) no longer break compilation.
- **Versioned derives compile in `no_std` crates** (PACK-008); generated
  code no longer names `::std`.
- **Canonical mode is injective for versioned structs** (re-audit
  PACK-R02). It accepted older-version frames, newer-version frames with
  arbitrary trailing bytes, and any content in a retired slot, so a hash
  or signature over a versioned value could be forged by appending bytes.
  A canonical decoder now reads a versioned struct only at its own
  version, rejects any unread body byte, and requires a retired slot to
  hold the encoding of the field's default (`NonCanonical` reasons
  `"versioned struct version"`, `"versioned body"`, `"retired field"`).
  Lenient decoding is unchanged and still reads every 1.x and 2.x frame.
- **serde sets** (re-audit PACK-R03). serde has no set type, so the
  adapter wrote `BTreeSet` in `Ord` order and `HashSet` in hash order
  (not deterministic), unlike the native derive, and canonical decoding
  could not check them. The new `pack_io::serde::sorted_set` helper
  writes the native, sorted set bytes and makes canonical mode check
  them; the docs no longer claim byte identity for unmarked sets.
- **Derive hygiene, generics** (re-audit PACK-R07): type parameters named
  `__D` / `__E` no longer collide with the derive's own.
- **Versioned encode checks `encoded_len` in debug builds** (re-audit
  PACK-R11): a hand-written `encoded_len` that disagrees with `serialize`
  now fails the encode with `SerialError::Custom` instead of silently
  writing a corrupt frame. The body is counted in the same pass that
  writes it, so debug-build encode stays linear in nesting depth
  (re-audit PACK-R15: an earlier separate counting pass was exponential).
- **Docs build without `std`** (re-audit PACK-R05): intra-doc links to
  `std`-only items broke `cargo doc --no-default-features`, and a serde
  doctest needed `std`.
- **Error payloads** (PACK-022): `UnknownVariant` reports the real tag
  (was `u64::MAX` for tags above `u32::MAX`); length errors report the
  real remaining input when known.

### Added

- **`#[pack_io(retired = N)]` and `#[pack_io(removed = N)]`** (see Fixed
  and Breaking Changes).
- **`Box<T>`** for every `T` (owned, view, encode), so recursive types
  such as `struct List { next: Option<Box<List>> }` derive directly.
- **Canonical decoding** (PACK-009). `Config::with_canonical(true)`
  accepts only the encoder's own bytes and returns
  `SerialError::NonCanonical` otherwise. It rejects:
  - non-canonical varints;
  - unsorted or duplicate map and set keys;

  - versioned structs at any version other than the reader's own,
    unread body bytes, and retired slots that do not hold the default.

  It is opt-in (see Changed) and works on the in-memory decoder.
  `IoDecoder::with_config` rejects it; buffer the frame with
  `decode_from_with_config` instead.
- **`Config::max_depth`, `Config::canonical`**, their `with_*` builders,
  `Config::DEFAULT_MAX_ALLOC` / `DEFAULT_MAX_DEPTH`.
- **`decode_with_config`, `decode_view_with_config`,
  `decode_from_with_config`** (PACK-021).
- **`SerialError` variants** `DepthLimitExceeded`, `NonCanonical`,
  `InvalidVersion`, `InvalidValue`, `InvalidConfig` and `Custom`, plus
  `SerialError::custom` for user code (PACK-021, PACK-022). Existing
  variants keep their discriminants.
- **`Serialize::encoded_len` / `encoded_len_slice`** and
  `pack_io::varint_len`: exact encoded size without encoding (PACK-014).
- **Provided trait methods.** All have defaults, so existing impls keep
  compiling:
  - `Decode::{remaining_hint, config, depth, enter_nested,
    leave_nested, consumed, bytes_read, unbacked, charge_unbacked,
    read_byte_cow}`;
  - `Deserialize::deserialize_array`;
  - `DeserializeView::{deserialize_view_many, deserialize_view_array}`.
- **`Decoder::read_borrowed`, `Decoder::config`, `IoDecoder::config`.**
- **Derive attributes**: `skip`, `default` / `default = "path"`,
  `with = "module"`, `crate = "path"` (PACK-019), explicit enum
  `index = N`.
- **`DeserializeView` derive on lifetime-less types** (PACK-020), so
  owned helper types nest inside borrowing views.
- **New impls** (PACK-028):
  - scalars and markers: `char`, `NonZero*`, `Duration`, `PhantomData<T>`;
  - collections: `VecDeque<T>`;
  - pointers: `Rc<T>`, `Rc<str>`, `Rc<[T]>`, `Arc<T>`, `Arc<str>`,
    `Arc<[T]>`, `Cow<'_, T>` (borrowed `Cow<str>` / `Cow<[u8]>` in
    views).

  Their wire encodings are in `docs/WIRE_FORMAT.md` §2.6 and §3.9.
- **serde adapter** (feature `serde`, PACK-012):
  - functions `pack_io::serde::{to_vec, to_encoder, from_slice,
    from_slice_with_config, from_decoder, from_decode}`;
  - the `compat` `with`-module and the `Compat<T>` wrapper;
  - the `sorted_set` serde `with`-module for set fields.

  serde-derived types encode byte-identically to the native derive;
  set fields need `sorted_set` for that (serde has no set type).
- **`bytes` integration** (feature `bytes`): `Serialize` / `Deserialize`
  for `Bytes` / `BytesMut`, `Encode` for `BytesMut`.
- **Tests**:
  - golden vectors written by pack-io 1.0.1 (46), plus 16 written by
    2.0.0 for the encodings added in 2.0 (`tests/golden_v2.rs`);
  - counting-allocator allocation-bound tests;
  - depth-limit tests, including a 1 MiB debug-build stack;
  - canonical-mode properties;
  - schema-evolution tests for retired and removed fields across reader
    versions;
  - 22 trybuild compile-fail cases and 4 compile-pass cases for the
    derive;
  - 28 unit tests in `pack-io-derive` (it had none);
  - serde and bytes suites, and hostile-reader stream tests.
- **Fuzzing**:
  - seven new targets: `decode_recursive`, `decode_io`,
    `decode_view_derive` (now with a retired field), `roundtrip` (now
    with `Box` recursion), `decode_serde`, `decode_zero_input` and
    `canonical_injective`;
  - seed inputs for all twenty targets;
  - `-malloc_limit_mb=64`, so an allocation the input does not back is
    reported as a crash.
- **Benchmarks**:
  - `benches/hot_paths.rs`, which uses only the 1.0 API, for
    before/after numbers;
  - `benches/features.rs`;
  - `benches/min_trials.rs`, a min-of-trials comparative harness for
    busy machines.

### Changed

- **Canonical decoding stays opt-in (re-evaluated for 2.0).**
  Correctness comes first, and lenient decoding is correct:
  - every valid encoding decodes to its value;
  - the spec permits lenient readers;
  - encoding is deterministic either way.

  Canonical mode adds injectivity, which matters only where the bytes
  themselves are trusted. It also needs the whole frame in memory, and
  it costs about 23 % on small-key map decoding. Turn it on per decoder
  where bytes are hashed or signed.
- **Map and set encoding** writes every entry into one scratch buffer
  and skips the sort when entries are already in order (PACK-015). It
  makes 2 growing allocations instead of one per entry, with the same
  bytes.
- **Versioned structs** compute the body length with `encoded_len`, write
  fields straight into the output on encode, and borrow the body on
  decode (PACK-016); same bytes.
- **`[T; N]`** encodes through `serialize_slice`, and `[u8; N]` decodes
  into a stack array with no heap allocation (PACK-017). `Vec<u8>` view
  fields copy in one step instead of byte by byte (PACK-018, view half).
- **Canonical mode costs lenient decoding nothing on integer arrays**
  (re-audit PACK-R04). Integer `deserialize_many` reads the canonical
  flag once per collection and runs a varint loop specialised for it;
  scalar varints consult the configuration only after a redundant zero
  byte. The 2.0.0 pre-release checked the flag on every varint, which,
  not code layout as first reported, caused its `Vec<u64>` decode
  regression.
- **`[T; N]` of any primitive integer, float, `bool` or `char`** decodes
  into a stack array (re-audit PACK-R09); other element types still go
  through a temporary `Vec`.
- **`pack-io-derive` moved to syn 3** (what serde_derive and rkyv use),
  so downstream builds no longer compile two syn majors.
- **`schema` feature** is documented as an alias for `derive` (PACK-029);
  it always was. Examples that used it now ask for `derive`.
- **CI** (PACK-024, PACK-025):
  - `actions/checkout@v7`, rustup-installed toolchains, and
    `permissions: contents: read`;
  - a nine-entry clippy feature matrix;
  - default, all, no_std, no_std+derive, no_std+serde,
    no_std+derive+serde+bytes, serde and bytes test runs on three OSes ×
    {stable, 1.85};
  - docs built with `-D warnings` for all features, no_std, and no_std
    with the integrations;
  - minimal direct dependency versions tested on the MSRV, a coverage
    job, and Miri over the unit tests;
  - every third-party action pinned to a commit SHA;
  - a real `no_std` target build and an MSRV all-targets build;
  - `cargo semver-checks`;
  - fuzz smoke runs with seeds and an allocation limit, pinned to the
    GNU target because a prebuilt musl `cargo-fuzz` cannot link the
    sanitizer;
  - the no-op loom job (which masked failure with `|| echo`) and the
    unused `loom` dev-dependency are gone.
- **Tooling** (PACK-026, PACK-027):
  - `rustfmt.toml` drops nightly-only options that stable ignored;
  - `deny.toml` has its header fixed and its license allow-list cut to
    the three licenses in the graph;
  - both crates publish an explicit `include` list;
  - the license links in `pack-io-derive/README.md` work on crates.io.
- **Dev-dependencies**:
  - criterion 0.5 → 0.7 (no rayon or crossbeam in the graph any more);
  - trybuild added;
  - the serde dev-dependency floor raised to 1.0.220 (the main
    dependency's), so `-Z direct-minimal-versions` resolves;
  - rkyv floor 0.8.18 (RUSTSEC-2026-0233, -0234 and -0235 affected the
    0.8.16 that local lockfiles held);
  - bincode stays on 2.0.1 as a benchmark reference; 3.0.0 is a
    tombstone release.
- **Docs**: README, `docs/API.md`, `docs/WIRE_FORMAT.md` (spec 1.3) and
  `docs/PERFORMANCE*.md` are rewritten for accuracy, and the performance
  baseline is re-measured. The rewrite fixes:
  - stale version references;
  - the nonexistent `View<T>` type and `loom_codec` test;
  - the "safe decode" and deprecation claims.

  `docs/MIGRATION.md` is new.

### Performance

These numbers come from `docs/PERFORMANCE_BASELINE.md`, comparing
1.0.1 with 2.0.0 on the same machine.

**Faster:**

- versioned struct encode −61 % and decode −41 %;
- `BTreeMap<String, _>` encode −64 %;
- `[u8; 32]` decode −95 %;
- 64 KiB `Vec<u8>` decode −37 %;
- owned log-record decode −19 %.

- `Vec<u64>` × 1000 decode −16 % (multi-byte values) and −20 % (small
  values) in a default release build, −37 % / −61 % with fat LTO
  (re-audit harness on Windows, rustc 1.99; the pre-release measured +10
  to +20 % here, see PACK-R04).

**Slower, beyond the 5 % budget, recorded with its cause:**

- `IoDecoder` plain struct +3 ns (+21 % on the WSL2 baseline machine,
  +8 to +13 % in the re-audit harness), from the depth limit.

### Re-audit fixes (pre-publication)

An independent re-audit of the 2.0.0 tree (2026-10-13) scored it 83/100
with one publish blocker. Every finding is addressed before publishing;
each fix has a regression test that fails on the pre-release tree.

| Finding | Fix |
|---|---|
| PACK-R01 (Medium, security) | Elements that occupy no input bytes are charged against `max_alloc` across the whole decode |
| PACK-R02 (Medium, publish blocker) | Canonical mode reads versioned structs only at the reader's version, with no trailing body bytes and default retired slots |
| PACK-R03 (Medium) | `pack_io::serde::sorted_set`; docs say exactly which serde shapes are byte-identical |
| PACK-R04 (Low, performance) | Integer arrays hoist the canonical check; `Vec<u64>` decode is faster than 1.0.1 |
| PACK-R05 (Low) | no_std docs and serde doctest fixed; both checked in CI |
| PACK-R06 (Low) | `retired` above `version` and `removed` more than one version ahead are compile errors |
| PACK-R07 (Low) | Generated generic parameters renamed |
| PACK-R08 (Low) | Mismatched Rust discriminants are compile errors |
| PACK-R09 (Low) | Primitive arrays decode on the stack |
| PACK-R10 (Low) | 4 KiB stream pre-size |
| PACK-R11 (Low) | Debug-build `encoded_len` check in versioned encode |
| PACK-R12 (Low) | `encode` docs state the 512-byte initial capacity |
| PACK-R13 (Info) | `peek_version` documents that it does not validate |
| PACK-R14 (Low) | Byte-equality test for streaming and in-memory varints at every boundary |

### Dependencies

- `serde` 1 (optional, feature `serde`, `default-features = false`,
  `alloc`): the adapter needs serde's data-model traits. It is
  maintained by dtolnay, uses no unsafe in the parts pack-io calls, and
  its MSRV is well under 1.85. pack-io's `std` feature enables
  `serde/std`.
- `bytes` 1.10 (optional, feature `bytes`, no default features): tokio's
  byte buffers, the type network code already passes around. It is
  maintained by the tokio project, has no transitive dependencies, and
  its MSRV is 1.39.
- `pack-io-derive`: `syn` 2 → 3; exact pin `=2.0.0`.

### Not changed

- **Wire format.** Every 1.x encoding of valid data is still produced
  byte for byte and decodes to the same value. This is checked by
  `tests/golden.rs` (46 vectors from 1.0.1) and by the 29
  byte-equivalence tests.
- **MSRV** stays 1.85.

---

## [1.0.1] - 2026-06-05

Patch release. Fixes a `cargo test` failure under the default feature set
that v1.0.0 shipped with — the `tests/integration_scenarios.rs` test
file uses `#[derive(Serialize)]` / `#[pack_io(version = N)]` but was
missing the `[[test]] required-features` entry in `Cargo.toml` that
gates it on the `schema` feature. Users running `cargo test` without
`--features schema` (the default invocation) saw compile errors.

### Fixed

- `tests/integration_scenarios.rs` now declares
  `required-features = ["schema", "std"]` in [`Cargo.toml`](./Cargo.toml), so
  it is correctly excluded from `cargo test` invocations that don't
  enable the `schema` feature. This matches the existing gates on the
  `derive` and `schema_evolution` tests.

### Changed

- [`.github/workflows/ci.yml`](./.github/workflows/ci.yml) now runs
  **three** test invocations per matrix cell: `cargo test` (default
  features), `cargo test --all-features`, and
  `cargo test --no-default-features` (no_std). The single
  `--all-features` invocation that shipped through v1.0.0 missed
  v1.0.1's bug because the integration test compiled successfully when
  `schema` was enabled. The new matrix covers the configurations real
  users actually pick, so a feature-gating mistake cannot reach a
  release again.

### Wire format / API

**Unchanged.** No source-breaking, behavioural, or wire-format change.

---

## [1.0.0] - 2026-06-05

The **stable** release. `pack-io 1.0.0` freezes the complete public
surface, wire format, and performance baseline for the entire `1.x`
line. The codec ships exactly as it has run since v0.6.0; v0.7–v0.9
added the hardening, integration coverage, and the frozen reference
data table this release locks in.

### Frozen for the `1.x` line

| Surface | Reference |
|---|---|
| Public API — every type, trait, free function, attribute, feature flag | [`docs/API.md` § Frozen public surface](./docs/API.md#frozen-public-surface) |
| Wire format — byte-level spec (version 1.2) | [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md) |
| Performance — comparative baseline | [`docs/PERFORMANCE_BASELINE.md`](./docs/PERFORMANCE_BASELINE.md) |

Source-breaking and wire-format-breaking changes are deferred to `2.x`.
A change exceeding 5 % regression on any row of the performance
baseline blocks the merge.

### Added

- **`#[derive(DeserializeView)]` on enums.** The derive previously
  rejected enums; v1.0 emits the same `varint(variant_index) ++ fields`
  wire shape as `#[derive(Deserialize)]`, with each variant's fields
  decoded via `DeserializeView` so borrow-shaped fields land as
  `&'a str` / `&'a [u8]` rather than `String` / `Vec<u8>`. Four new
  tests in [`tests/derive.rs`](./tests/derive.rs) cover unit, tuple,
  named-field variants plus the unknown-variant rejection path.

### Changed

- Status banner across [README](./README.md), [`docs/API.md`](./docs/API.md),
  and [`docs/PERFORMANCE_BASELINE.md`](./docs/PERFORMANCE_BASELINE.md)
  flipped from "beta — bug-fixes-only" to **"stable (v1.0.0)"**.
- README roadmap section replaced with a `## Stability contract`
  section listing the three frozen surfaces and pointing at their
  reference docs.
- README + API.md + crate-level rustdoc + install snippets bumped from
  `0.9` → `1`.
- API.md `Compatibility & semver` section rewritten — drops the pre-1.0
  / post-1.0 framing in favour of strict SemVer language for the `1.x`
  line.
- API.md `DeserializeView` derive section updated to document enum
  support and the permanent single-lifetime restriction.
- WIRE_FORMAT.md spec banner updated — drops "Breaking changes inside
  the `0.x` series are called out…" framing in favour of "Wire-format-
  breaking changes are deferred to `2.x`".
- PERFORMANCE_BASELINE.md banner rewritten as the frozen reference
  with the contractual 5 % regression policy stated up front.
- `fuzz/README.md` target table flattened — the 13 targets no longer
  carry per-version "(v0.9)" markers since they're all just "the fuzz
  targets" in the stable release.
- Stale `(v0.3.0)` / "new in v0.3" / "currently:" / "pack-io v0.2"
  markers swept out of crate-level rustdoc, `src/codec.rs` doc
  comments, and three example source files.

### Wire format

**Unchanged.** Every payload encoded by any pre-1.0 release decodes
identically under v1.0. Spec version remains `1.2`.

### Performance baseline (the numbers v1.0 ships)

High-fidelity Criterion medians, 100 samples × 10 s measurement × 2 s
warmup. Frozen as the contractual reference for the entire `1.x` line.

| Workload | pack-io | bincode | postcard | rkyv | Position |
|---|---:|---:|---:|---:|---|
| encode/log_record | **37.9 ns** | 39.4 ns | 232.6 ns | 112.6 ns | pack-io fastest |
| decode_owned/log_record | 158.9 ns | 165.1 ns | 285.1 ns | **154.6 ns** | rkyv 1.03× faster |
| decode_view/log_record | 34.7 ns | — | — | **12.0 ns** | rkyv 3× faster (by design) |
| `u64` round-trip | 22.3 ns | **20.6 ns** | — | — | bincode 1.08× faster |
| 64-byte `String` owning | **44.7 ns** | 48.8 ns | — | — | pack-io fastest |
| 64-byte `&str` view | **5.2 ns** | — | — | — | uncontested |
| 4 KiB `Vec<u8>` decode | **59.7 ns** | 63.0 ns | — | — | pack-io fastest |

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
cd fuzz && cargo +nightly check
```

Test counts at this tag (stable, `--all-features`): **278 total**, all
passing (was 274 in v0.9 — +4 enum DeserializeView tests in
`tests/derive.rs`). All ten example programs run end-to-end. All 13
fuzz targets compile cleanly under nightly.

---

## [0.9.0] - 2026-06-04

The **beta** release. v0.9.x is a bug-fixes-only window; this release
itself ships zero new public API and zero wire-format changes. What
v0.9.0 adds is the broader-testing infrastructure that pre-RC software
needs and the **v1.0 performance baseline** committed as the canonical
reference for post-1.0 regression detection.

Honest caveat: the roadmap defines the alpha→beta promotion as gated on
"a stable stretch with no outstanding bugs from the v0.8.x line", and
no real-consumer bug reports have come in yet. v0.9.0 is the ceremonial
promotion that opens the bug-fixes-only window; the substantive content
is the testing + baseline work below.

### Status

- **Crate status:** beta. v0.9.x is **bug-fixes-only**.
- **Performance baseline:** frozen at [`docs/PERFORMANCE_BASELINE.md`](./docs/PERFORMANCE_BASELINE.md).
  Any post-1.0 change exceeding 5 % regression on any row blocks the merge.
- **RC target:** v0.9.5+, critical fixes + doc polish only.
- **Stable target:** v1.0.0.

### Added

- **5 new fuzz targets** for broader continuous coverage of the decode
  surface:
  - [`decode_btreemap`](./fuzz/fuzz_targets/decode_btreemap.rs) —
    `BTreeMap<u64, String>` ordered-map non-`std` path
  - [`decode_btreeset`](./fuzz/fuzz_targets/decode_btreeset.rs) —
    `BTreeSet<String>` with per-element UTF-8 validation
  - [`decode_hashset`](./fuzz/fuzz_targets/decode_hashset.rs) —
    `HashSet<u32>` preallocation cap
  - [`decode_view_bytes`](./fuzz/fuzz_targets/decode_view_bytes.rs) —
    zero-copy `&[u8]` decode (no UTF-8 path)
  - [`decode_view_collection`](./fuzz/fuzz_targets/decode_view_collection.rs) —
    `Vec<&str>` collection of borrows
  Total continuous coverage now **13 fuzz targets** (was 8 in v0.7).
- **CI fuzz runtime bumped 30 s → 60 s per target.** Every push to
  `main` now exercises ~13 minutes of continuous fuzzing across all
  targets vs ~4 minutes previously. Same Linux nightly + ASAN
  instrumentation as v0.7.
- [`docs/PERFORMANCE_BASELINE.md`](./docs/PERFORMANCE_BASELINE.md) —
  the canonical v1.0 performance reference. High-fidelity Criterion
  medians (100 samples × 10 s measurement window × 2 s warmup) for
  every workload in the comparative suite. The 10 s window confirms
  the v0.6 / v0.7 quick-run numbers within ~3 % across every row.
  Regression policy: post-1.0 changes exceeding 5 % on any row block
  the merge.

### Changed

- README + API.md status markers updated from "alpha" to "beta".
- `fuzz/README.md` target table extended with the 5 new entries.
- Roadmap row for v0.9 marked shipped; v0.9.5+ scoped to RC; v1.0 next.

### Performance baseline summary

(Full table + methodology + environment in
[`docs/PERFORMANCE_BASELINE.md`](./docs/PERFORMANCE_BASELINE.md).)

| Workload | pack-io | nearest competitor | Position |
|---|---:|---|---|
| encode/log_record | **37.9 ns** | bincode 39.4 ns | pack-io fastest |
| decode_owned/log_record | 158.9 ns | rkyv 154.6 ns | rkyv 1.03× faster, pack-io 2nd |
| decode_view/log_record | 34.7 ns | rkyv 12.0 ns | rkyv 3× faster (by design) |
| `u64` round-trip | 22.3 ns | bincode 20.6 ns | bincode 1.08× faster |
| 64-byte `String` owning | **44.7 ns** | bincode 48.8 ns | pack-io fastest |
| 64-byte `&str` view | **5.2 ns** | n/a | uncontested |
| 4 KiB `Vec<u8>` decode | **59.7 ns** | bincode 63.0 ns | pack-io fastest |

### Wire format

**Unchanged.** Every v0.8 payload decodes identically under v0.9. Spec
version remains `1.2`.

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
```

Test counts at this tag (stable, `--all-features`): **274 total**, all
passing (unchanged from v0.8 — v0.9 is bug-fixes-only and we have no
bugs to fix yet, only broader testing infrastructure).

---

## [0.8.0] - 2026-06-04

The **alpha-integration window** opens. v0.8.0 carries zero new public
API and zero wire-format changes — what it adds is the documentation,
status markers, and consumer-shape test / example coverage that signal
"pack-io is ready for the first real downstream crates to wire it up".
Point releases in the v0.8.x line will track bugs surfaced by real
consumer integration; the v0.7 API freeze holds throughout.

### Status

- **Crate status:** alpha. The first real consumers (`network-protocol`,
  `wire-codec`, Hive DB, `raft-io` log entries, future `event-stream`
  framing) start wiring pack-io in. Bugs they surface become v0.8.x
  point releases. No new public API; the v0.7 freeze holds.
- **Beta target:** v0.9.0, gated on a stable stretch of no outstanding
  bugs.
- **RC target:** v0.9.5+, critical fixes plus doc polish only.
- **Stable target:** v1.0.0.

### Added

- [`tests/integration_scenarios.rs`](./tests/integration_scenarios.rs) —
  **8 consumer-shape integration tests** that exercise the substrate
  the way real consumers will:
  - Length-framed message exchange (sender encodes, receiver decodes
    from the same byte stream, three messages in a single buffer).
  - Versioned protocol handshake — `v1↔v2` cross-decode in both
    directions, with the v1 server skipping v2-only fields cleanly.
  - Streaming event log over `IoEncoder` / `IoDecoder` wrapping
    `BufWriter` / `BufReader`, multiple event types via an `Event`
    enum.
  - Zero-copy request inspection (`decode_view`) with a **runtime
    pointer-equality check** proving the view's `&str` / `&[u8]`
    fields point inside the original wire buffer, not into
    freshly-allocated heap memory.
  - Tight `Config::max_alloc` enforcement against a hostile-payload
    request handler.
  - Single-shot `encode_into` / `decode_from` round-trip through a
    `Cursor`.
- [`examples/protocol_handshake.rs`](./examples/protocol_handshake.rs) —
  versioned protocol handshake walkthrough that runs all four
  `v1 ↔ v2` cross-decode combinations end-to-end and prints what each
  side observes.
- [`examples/event_log.rs`](./examples/event_log.rs) — WAL-style
  append-only event log written to a tempfile via `IoEncoder<BufWriter<File>>`
  and replayed via `IoDecoder<BufReader<File>>`, demonstrating the
  enum wire format with four variant shapes (unit / named / tuple /
  mixed).
- README + API.md status markers updated from "pre-1.0, API frozen" to
  "alpha — integration window open".

### Changed

- Roadmap entry for v0.8 marked shipped; v0.9.x scoped to beta → RC.

### Wire format

**Unchanged.** Every v0.7 payload decodes identically under v0.8. Spec
version remains `1.2`.

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
```

Test counts at this tag (stable, `--all-features`): **274 total**, all
passing (was 266 in v0.7, +8 integration scenarios).

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

[Unreleased]: https://github.com/jamesgober/pack-io/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/jamesgober/pack-io/compare/v1.0.1...v2.0.0
[1.0.1]: https://github.com/jamesgober/pack-io/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/jamesgober/pack-io/compare/v0.9.0...v1.0.0
[0.9.0]: https://github.com/jamesgober/pack-io/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/jamesgober/pack-io/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/jamesgober/pack-io/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/jamesgober/pack-io/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/jamesgober/pack-io/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/jamesgober/pack-io/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/jamesgober/pack-io/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/jamesgober/pack-io/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/pack-io/releases/tag/v0.1.0
