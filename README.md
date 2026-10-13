<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <b>pack-io</b>
    <br>
    <sub>
        <sup>COMPACT BINARY WIRE FORMAT</sup>
    </sub>
</h1>

<div align="center">
    <a href="https://crates.io/crates/pack-io"><img alt="Crates.io" src="https://img.shields.io/crates/v/pack-io"></a>
    <a href="https://crates.io/crates/pack-io" alt="Download pack-io"><img alt="Crates.io Downloads" src="https://img.shields.io/crates/d/pack-io?color=%230099ff"></a>
    <a href="https://docs.rs/pack-io" title="pack-io Documentation"><img alt="docs.rs" src="https://img.shields.io/docsrs/pack-io"></a>
    <a href="https://github.com/jamesgober/pack-io/actions"><img alt="GitHub CI" src="https://github.com/jamesgober/pack-io/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/rust-lang/rfcs/blob/master/text/2495-min-rust-version.md" title="MSRV"><img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.85%2B-blue"></a>
</div>

<br>

<div align="left">
    <p>
        <strong>pack-io</strong> is a <b>compact binary wire format</b> for Rust built around three properties existing serialization crates split across three separate libraries: <b>speed</b>, <b>schema evolution</b>, and <b>zero-copy deserialization</b>. It is engineered as the serialization substrate underneath <a href="https://github.com/jamesgober/network-protocol"><code>network-protocol</code></a>, <a href="https://github.com/jamesgober/wire-codec"><code>wire-codec</code></a>, and Hive DB - so it has to be fast, deterministic, schema-aware, and safe under untrusted input.
    </p>
    <p>
        Existing crates each cover part of the problem. <code>bincode</code> is fast but has no schema evolution, and its development has ceased. <code>rkyv</code> gives zero-copy but requires alignment discipline at the use site. <code>postcard</code> is embedded-focused and lean. None of them own all three properties as a single coherent contract. <code>pack-io</code> does, behind a small, predictable wire format you can read with a spec instead of with the source code.
    </p>
    <p>
        The common-case API is one line - <code>encode(&amp;value)</code> and <code>decode::&lt;T&gt;(&amp;bytes)</code> - and that path is the fast path. Derives, the zero-copy view API, and the serde and <code>bytes</code> integrations live behind feature flags so the default build stays small.
    </p>
    <br>
    <hr>
    <p>
        <strong>MSRV is 1.85+</strong> (Rust 2024 edition). <code>no_std</code>-capable. Deterministic encoding. <code>#![forbid(unsafe_code)]</code>.
    </p>
    <blockquote>
        <strong>Status: stable (v2.0.0).</strong> The public API listed in <a href="./docs/API.md#frozen-public-surface"><code>docs/API.md</code></a> is the <code>2.x</code> contract: items are only ever added within 2.x. The wire format (<a href="./docs/WIRE_FORMAT.md"><code>docs/WIRE_FORMAT.md</code></a>, spec version 1.3) is unchanged since 1.0: every 2.x decoder reads every 1.x or 2.x encoding of valid data, checked against golden vectors written by 1.0.1. 2.0 is a major release because decoding now enforces limits and the derive rejects misuse that 1.x accepted — see the <a href="./docs/MIGRATION.md">migration guide</a>. The performance baseline at <a href="./docs/PERFORMANCE_BASELINE.md"><code>docs/PERFORMANCE_BASELINE.md</code></a> is the reference for the 5 % regression policy.
    </blockquote>
</div>


<hr>
<br>

## Why pack-io

Existing crates each cover a slice of the problem; none of them own all three properties together.

| Crate       | Speed | Schema evolution | Zero-copy decode | Wire-format spec |
|-------------|:-----:|:----------------:|:----------------:|:----------------:|
| `bincode`   |   ✓   |        —         |        —         |        —         |
| `rkyv`      |   ✓   |        —         |        ✓ *       |        —         |
| `postcard`  |   ✓   |        —         |        —         |        ✓         |
| **`pack-io`** | **✓** |       **✓**      |       **✓**      |       **✓**      |

<sub>* `rkyv` requires alignment discipline at every use site; the cost is paid by the caller, not the codec.</sub>

The contract is the same wire format on every supported platform, the same bytes for the same value every time, and a decoder that cannot be made to panic, overflow the stack, or allocate memory the input does not back.

### Speed, with numbers

Measured for v2.0.0 on an AMD Ryzen 9 9950X3D (Ubuntu under WSL2, rustc 1.99, one pinned core), as the fastest of 300 short trials — the machine was shared with other work, which made Criterion's means swing by tens of percent. Method, environment and the full tables: [`docs/PERFORMANCE_BASELINE.md`](./docs/PERFORMANCE_BASELINE.md). Reproduce with `cargo bench --bench min_trials --features derive`.

| Workload (ns, lower is better) | pack-io | bincode | postcard | rkyv |
|---|---:|---:|---:|---:|
| struct encode | 24.4 | **24.0** | 134.0 | 53.6 |
| struct decode (owned) | 57.2 | 153.2 | 134.3 | **55.1** |
| struct decode (zero-copy) | 25.4 | — | — | **12.8** |
| `u64` round-trip | **5.7** | 7.7 | 6.6 | — |
| 64-byte `String` round-trip | **16.6** | 19.8 | 32.4 | — |
| 64-byte `&str` view | **5.4** | — | — | — |
| 4 KiB `Vec<u8>` decode | **31.3** | 48.8 | 1,137 | — |

pack-io is the fastest of the four on the `u64` and `String` round-trips, the `&str` view and `Vec<u8>` decode; it ties bincode on struct encode and rkyv on owned struct decode. rkyv's archived access is about 2× faster than pack-io's zero-copy view — rkyv reads a fixed memory layout, pack-io walks varints as its spec defines, by design. Ratios depend on platform and allocator (on Windows the 1.0 runs had bincode's owned decode much closer to pack-io's), so benchmark your own workload. Regression policy: a change exceeding 5 % on any row blocks the merge; analysis in [`docs/PERFORMANCE.md`](./docs/PERFORMANCE.md).

<br>
<hr>
<br>

## What it does

- **Compact binary encoding** of Rust values into a small, predictable wire format
- **Schema-versioned messages** - append fields without breaking older or newer peers; remove fields with a documented reader-first rollout
- **Zero-copy deserialization** for `&[u8]` / `&str` / length-prefixed slices when the input lives long enough
- **Deterministic output** - the same value always produces the same bytes; an opt-in canonical mode makes the decoder accept only those bytes
- **Safe under untrusted input** - allocation bounded by the input and a per-value cap, nesting bounded by a depth limit, no panics on malformed bytes
- **Runtime-agnostic** - synchronous codec, usable from any context

<br>

## Features

- **Compact** — varint integers, length-prefixed strings and byte runs, no field names on the wire
- **Schema evolution** — `#[pack_io(version, since)]` to add fields, `retired` to drop one safely, `peek_version` for dispatch
- **Zero-copy decode** — `DeserializeView` borrows `&str` / `&[u8]` fields straight from the input
- **Deterministic** — canonical map / set ordering; `Config::with_canonical(true)` for hash-, signature- and content-address-safe decoding
- **Safe defaults** — `max_alloc` (1 GiB) and `max_depth` (128) limits, input-bounded allocation, versioned bodies decoded under the caller's limits
- **`no_std`-capable** — `core` + `alloc`, derives included
- **Derive macro** — `#[derive(Serialize, Deserialize, DeserializeView)]` with `skip`, `default`, `with`, `crate`, and explicit enum `index` attributes (feature `derive`)
- **serde adapter** — any `serde` type on the pack-io wire, byte-identical to the native derive for the same shape (feature `serde`)
- **`bytes` integration** — `Bytes` / `BytesMut` values, and `BytesMut` as an encode sink (feature `bytes`)

<hr>
<br>

## Stability contract

| Frozen for `2.x` | Reference |
|---|---|
| Public API — types, traits, free functions, attributes, feature flags (additions only) | [`docs/API.md` § Frozen public surface](./docs/API.md#frozen-public-surface) |
| Wire format — byte-level spec (version 1.3) | [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md) |
| Performance — comparative baseline | [`docs/PERFORMANCE_BASELINE.md`](./docs/PERFORMANCE_BASELINE.md) |

Backwards-compatible additions (new variants on the `#[non_exhaustive]` `SerialError` enum, new trait methods with default implementations, new built-in impls for `core` / `alloc` types) land in `2.x` minor releases. Source-breaking and wire-format-breaking changes wait for a new major version. Upgrading from 1.x: [`docs/MIGRATION.md`](./docs/MIGRATION.md).

Per-release history is in [`CHANGELOG.md`](./CHANGELOG.md); detailed release notes live in [`docs/release/`](./docs/release/).

<hr>
<br>

## Installation

```toml
[dependencies]
pack-io = "2"

# With derive macros (including the schema-evolution attributes):
pack-io = { version = "2", features = ["derive"] }

# serde adapter / bytes integration:
pack-io = { version = "2", features = ["derive", "serde", "bytes"] }

# no_std build (core + alloc; no streaming I/O, no HashMap / HashSet):
pack-io = { version = "2", default-features = false, features = ["derive"] }
```

The `schema` feature from earlier releases still works; it is an alias for `derive`.

<br>

## API surface

Three tiers cover every use case. See [`docs/API.md`](./docs/API.md) for the complete reference and [`docs/WIRE_FORMAT.md`](./docs/WIRE_FORMAT.md) for the normative byte-level spec.

### Tier 1 — the lazy path

```rust
use pack_io::{encode, decode};

let bytes: Vec<u8> = encode(&(7_u64, true, String::from("hello"))).unwrap();
let back: (u64, bool, String) = decode(&bytes).unwrap();
assert_eq!(back, (7, true, String::from("hello")));
```

### Tier 2a — the in-memory `Encoder` / `Decoder`

Re-use a single `Vec<u8>` across many encodes; read several values from one buffer. Configuration is validated once, at construction time.

```rust
use pack_io::{Encoder, Decoder, Config};

let mut enc = Encoder::new();
enc.write(&7_u64).unwrap();
enc.write(&"hello").unwrap();
let bytes = enc.into_inner();

let cfg = Config::new().with_max_alloc(16 * 1024);
let mut dec = Decoder::with_config(&bytes, cfg).unwrap();
let n: u64 = dec.read().unwrap();
let s: String = dec.read().unwrap();
assert_eq!((n, s.as_str()), (7, "hello"));
```

### Tier 2b — the streaming `IoEncoder<W>` / `IoDecoder<R>`

Write directly into any `std::io::Write`, read from any `std::io::Read`. Gated on the default `std` feature. The streaming decoder grows its buffers only as bytes arrive, so a peer that declares a huge length and stalls costs one small chunk.

```rust
use pack_io::{IoEncoder, IoDecoder, encode_into, decode_from};
use std::io::Cursor;

// Single-shot helpers (encode_into / decode_from) over any Read / Write:
let mut sink: Vec<u8> = Vec::new();
encode_into(&(42_u64, "hello"), &mut sink).unwrap();
let back: (u64, String) = decode_from(&mut Cursor::new(sink)).unwrap();
assert_eq!(back, (42, "hello".to_string()));

// Or hold an encoder / decoder for multi-value streams:
let mut buf: Vec<u8> = Vec::new();
{
    let mut enc = IoEncoder::new(&mut buf);
    enc.write(&1_u64).unwrap();
    enc.write(&2_u64).unwrap();
}
let mut dec = IoDecoder::new(Cursor::new(buf));
let a: u64 = dec.read().unwrap();
let b: u64 = dec.read().unwrap();
assert_eq!((a, b), (1, 2));
```

### Tier 3 — derive on your own types

`#[derive(Serialize, Deserialize)]` writes the boilerplate. Works on every struct shape (named, tuple, unit), every enum variant shape, and on generic types.

```rust
use pack_io::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Account {
    id: u64,
    handle: String,
    flags: Vec<String>,
    active: bool,
    #[pack_io(skip)]
    cached_score: Option<f64>, // never on the wire; decodes as None
}

#[derive(Serialize, Deserialize)]
enum Event {
    Heartbeat,
    Login { user: u64, ip: String },
    Error(u32, String),
    #[pack_io(index = 100)]
    Shutdown, // explicit wire tag, independent of declaration order
}
```

Enums encode as `varint(variant_index) ++ fields`. Indices follow declaration order unless pinned with `#[pack_io(index = N)]`, so either **append new variants to the end** or pin the indices. Attributes: `skip`, `default` / `default = "path"`, `with = "module"`, `crate = "path"` (for crates that re-export pack-io), and `index = N`; anything unknown is a compile error. See [`docs/API.md` § Derive attributes](./docs/API.md#derive-attributes).

### Schema evolution — append-only forward and backward compatibility

Add `#[pack_io(version = N)]` to your type and `#[pack_io(since = N)]` to additive fields. Old encoders write what they know; new decoders read what's there and `Default::default()` the rest. New encoders write everything; old decoders read what they know and skip the trailing bytes inside the length-framed body.

```rust
use pack_io::{Serialize, Deserialize, encode, decode, peek_version};

#[derive(Serialize, Deserialize)]
#[pack_io(version = 1)]
struct MessageV1 { id: u64, text: String }

#[derive(Serialize, Deserialize)]
#[pack_io(version = 2)]
struct MessageV2 {
    id: u64,
    text: String,
    #[pack_io(since = 2)]
    timestamp: Option<u64>,
}

let bytes = encode(&MessageV1 { id: 7, text: "hello".into() }).unwrap();
assert_eq!(peek_version(&bytes).unwrap(), 1);

// v2 reads v1 cleanly: `timestamp` defaults to None.
let upgraded: MessageV2 = decode(&bytes).unwrap();
assert_eq!(upgraded.timestamp, None);

// And v1 reads v2 cleanly: the trailing timestamp bytes are skipped.
let bytes = encode(&MessageV2 {
    id: 7,
    text: "hello".into(),
    timestamp: Some(42),
}).unwrap();
let downgraded: MessageV1 = decode(&bytes).unwrap();
assert_eq!(downgraded.id, 7);
```

Versioned structs wrap their body in a `varint(version) ++ varint(body_len) ++ body` frame; non-versioned structs keep the plain field-concatenation encoding. The choice is per-type, opt-in via the attribute. The body is decoded under the caller's limits, and both the owning and the zero-copy decoders read it.

**Dropping a field** has two forms. `#[pack_io(retired = N)]` keeps the field's slot on the wire and writes its default from version `N`: every reader, including ones built before the change, stays aligned and simply sees the default, and new readers ignore whatever is in the slot. That is the safe choice and costs about one byte. `#[pack_io(removed = N)]` drops the bytes entirely (the 1.x `deprecated` encoding, so 1.x payloads keep decoding); because readers that predate the removal cannot detect it, roll readers out first. 1.x `deprecated = N` is a compile error in 2.0 that names both options; write `removed = N` to keep the exact 1.x bytes. Full normative spec: [`docs/WIRE_FORMAT.md §3.8`](./docs/WIRE_FORMAT.md#38-versioned-structs).

```rust
use pack_io::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
#[pack_io(version = 3)]
struct Account {
    id: u64,
    #[pack_io(retired = 3)]
    legacy_token: String, // written as "" from version 3; old readers see ""
    #[pack_io(since = 2)]
    handle: String,
}
```

### Tier 3 — zero-copy views

`#[derive(DeserializeView)]` plus the [`decode_view`] free function give you a parallel "borrowed" decode path. `&'a str` and `&'a [u8]` fields point directly into the input buffer — no per-field allocation, the borrow checker enforces the lifetime. Types without a lifetime can derive it too, so owned helper types nest inside views.

```rust
use pack_io::{Serialize, DeserializeView, decode_view, encode};

#[derive(Serialize)]
struct OwnedMsg { id: u64, text: String, payload: Vec<u8> }

#[derive(DeserializeView)]
struct ViewMsg<'a> { id: u64, text: &'a str, payload: &'a [u8] }

let bytes = encode(&OwnedMsg {
    id: 7,
    text: "borrowed".into(),
    payload: vec![1, 2, 3],
}).unwrap();

let view: ViewMsg<'_> = decode_view(&bytes).unwrap();
assert_eq!(view.text, "borrowed");  // points into `bytes`
```

On the representative borrow-heavy record (`u64 + u8 + String + Vec<String> + Vec<u8>`), the view decode takes 25 ns against 57 ns for the owning decode on the measurement machine (about 2.3× faster), and a 64-byte `&str` view takes 5.4 ns. The gap grows with the number and size of string and byte fields. Reproduce with `cargo bench --bench codec_bench --features derive`.

### Decoding untrusted input

Every decoder enforces, on any byte sequence: no panic, no read past the input, no single value above `max_alloc` (default 1 GiB), no allocation the input cannot back, and no nesting deeper than `max_depth` (default 128 derived types). Tighten the limits to your protocol, and turn on canonical mode where bytes are hashed or signed:

```rust
use pack_io::{Config, SerialError, decode_with_config};
use std::collections::BTreeMap;

let cfg = Config::new()
    .with_max_alloc(64 * 1024)
    .with_max_depth(32)
    .with_canonical(true);

// An unsorted map with a duplicate key: a lenient decoder accepts it,
// canonical mode rejects it.
let bytes = [3, 2, 20, 1, 10, 2, 99];
assert!(pack_io::decode::<BTreeMap<u32, u32>>(&bytes).is_ok());
assert_eq!(
    decode_with_config::<BTreeMap<u32, u32>>(&bytes, cfg),
    Err(SerialError::NonCanonical { reason: "map key order" }),
);
```

### serde adapter (feature `serde`)

```rust
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
struct Point { x: i32, y: i32 }

let bytes = pack_io::serde::to_vec(&Point { x: 3, y: -7 }).unwrap();
assert_eq!(bytes, pack_io::encode(&(3_i32, -7_i32)).unwrap()); // same wire as the native derive
let back: Point = pack_io::serde::from_slice(&bytes).unwrap();
assert_eq!(back, Point { x: 3, y: -7 });
```

serde-derived types encode to the same bytes as the native derive for the same shape, so the two can be mixed on one wire. `deserialize_any`-based serde features (untagged enums, `flatten`) are not supported, because the format is not self-describing. Details: [`docs/API.md` § serde adapter](./docs/API.md#serde-adapter-feature-serde).

### If you need to hand-roll: the [`Serialize`] / [`Deserialize`] traits

Both are generic over the `Encode` / `Decode` behaviour traits — one impl works through every encoder flavour the crate ships (in-memory **and** streaming).

```rust
use pack_io::{Decode, Deserialize, Encode, Result, Serialize};

struct Point { x: i32, y: i32 }

impl Serialize for Point {
    fn serialize<E: Encode + ?Sized>(&self, enc: &mut E) -> Result<()> {
        self.x.serialize(enc)?;
        self.y.serialize(enc)
    }
}

impl Deserialize for Point {
    fn deserialize<D: Decode + ?Sized>(dec: &mut D) -> Result<Self> {
        Ok(Point {
            x: i32::deserialize(dec)?,
            y: i32::deserialize(dec)?,
        })
    }
}
```

### Supported types

| Group | Types |
|---|---|
| Unsigned integers | `u8`, `u16`, `u32`, `u64`, `u128`, `usize`, `NonZeroU*` |
| Signed integers | `i8`, `i16`, `i32`, `i64`, `i128`, `isize`, `NonZeroI*` |
| Floats | `f32`, `f64` |
| Bool / char / unit | `bool`, `char`, `()`, `PhantomData<T>` |
| Strings | `String`, `&str` (encode + view), `Box<str>`, `Rc<str>`, `Arc<str>`, `Cow<str>` (borrowed in views) |
| Bytes | `Vec<u8>`, `&[u8]` (encode + view), `Cow<[u8]>`, `bytes::Bytes` / `BytesMut` *(bytes)* |
| Sequences | `Vec<T>`, `&[T]` (encode), `Box<[T]>`, `VecDeque<T>`, `[T; N]` |
| Tuples | arity 1 through 12 |
| Sums | `Option<T>`, `Result<T, E>` |
| Maps | `BTreeMap<K, V>`, `HashMap<K, V>` *(std)* |
| Sets | `BTreeSet<T>`, `HashSet<T>` *(std)* |
| Time | `Duration` |
| Pointers | `&T` (encode), `Box<T>`, `Rc<T>`, `Arc<T>`, `Cow<T>` — transparent |
| User types | any struct / enum with `#[derive(Serialize, Deserialize)]` *(derive)* |
| Zero-copy types | any struct / enum with `#[derive(DeserializeView)]` *(derive)* |
| serde types | any `serde::Serialize` / `Deserialize` type through `pack_io::serde` *(serde)* |

`Box<T>` encodes exactly like `T`, so recursive types such as `struct List { value: u8, next: Option<Box<List>> }` derive directly; their depth is bounded by `max_depth`.

### Canonical map / set encoding (the determinism contract)

Maps and sets (`BTreeMap`, `HashMap`, `BTreeSet`, `HashSet`) are encoded with entries sorted lexicographically by their **encoded key bytes**. A `HashMap` and a `BTreeMap` holding the same logical data therefore encode to **identical bytes**, regardless of insertion order or build-flag-dependent hash randomisation. With `Config::with_canonical(true)` the decoder enforces that order (and shortest-form varints), so the bytes identify the value. Canonical decoding stays opt-in in 2.0: lenient decoding already returns the right value for every valid encoding, enforcing canonical form needs the whole frame in memory (the streaming `IoDecoder` rejects it), and it costs about 25 % on small-key map decoding, which only the users who hash or sign encoded bytes need to pay. Full normative spec: [`docs/WIRE_FORMAT.md` §4](./docs/WIRE_FORMAT.md#4-maps-and-sets).

<br>

## Invariants

- **Round-trip integrity** — `decode(encode(v)) == v` for every supported type.
- **Determinism** — the same value always produces the same bytes; no map-iteration-order leaks, no time-dependence, no platform-dependence.
- **Safe decode** — no panic, no read past the input, allocation bounded by the input and `max_alloc`, nesting bounded by `max_depth`, on any byte sequence.
- **Wire-format stability** — unchanged since 1.0; any 2.x decoder reads any 1.x or 2.x encoding of valid data.

As of 2.0.0 these are enforced by 400 unit and integration tests and 76 doc tests (`cargo test --workspace --all-features`): property tests for round-trip, determinism, canonical-mode injectivity and `encoded_len`; 46 golden vectors written by pack-io 1.0.1 and 16 for the encodings added in 2.0; a counting-allocator suite that bounds peak memory on hostile input; recursion-depth tests; and 23 compile-fail / compile-pass cases for the derive. Eighteen `cargo-fuzz` targets run in CI with a 64 MiB allocation limit ([`fuzz/README.md`](./fuzz/README.md)).

<br>

## Testing

```bash
# Stable + MSRV (1.85) on Linux / macOS / Windows, full feature matrix
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# Supply chain and API compatibility
cargo audit
cargo deny check
cargo semver-checks --package pack-io

# Fuzzing (nightly, Linux / WSL), see fuzz/README.md
cd fuzz && cargo +nightly fuzz run --target x86_64-unknown-linux-gnu roundtrip corpus/roundtrip seeds/roundtrip -- -max_total_time=60 -malloc_limit_mb=64

# Benchmarks
cargo bench --features derive --bench comparative
cargo bench --features derive --bench hot_paths
cargo bench --features derive,serde --bench features
```

<br>

## Examples

Each example is self-contained.

```bash
cargo run --example basic_roundtrip --release                        # Tier-1 encode/decode of a tuple
cargo run --example primitive_tour --release                         # one encoded value per primitive type
cargo run --example reuse_buffer --release                           # Tier-2 Encoder + multi-value Decoder
cargo run --example collections_tour --release                       # Vec / HashMap / BTreeMap / sets, canonical encoding
cargo run --example streaming_io --release                           # IoEncoder / IoDecoder to a file
cargo run --example derive_intro --features derive --release         # #[derive(Serialize, Deserialize)] on structs + enums
cargo run --example view_zero_copy --features derive --release       # #[derive(DeserializeView)] borrows from the buffer
cargo run --example schema_evolution --features derive --release     # v1 <-> v2 cross-version decode walkthrough
cargo run --example protocol_handshake --features derive --release   # all four version combinations of a real handshake
cargo run --example event_log --features derive --release            # WAL-style multi-event log written to a tempfile and replayed
```

<hr>
<br>

## Cross-Platform Support

**Tier 1 Support:**
- ✅ Linux (x86_64, aarch64)
- ✅ macOS (x86_64, Apple Silicon)
- ✅ Windows (x86_64)

Encoding is byte-deterministic across all three; the CI matrix runs the test suite on each, on stable and MSRV, and also builds for a bare-metal `no_std` target (`thumbv7em-none-eabihf`). Platform-specific behaviour is forbidden in the codec — there is no `#[cfg(target_os = …)]` branch on the encode or decode path.

<br>

## Where It Fits

`pack-io` is the serialization substrate under [`network-protocol`](https://github.com/jamesgober/network-protocol), [`wire-codec`](https://github.com/jamesgober/wire-codec), and Hive DB. It is consumed by [`raft-io`](https://github.com/jamesgober/raft-io) for log entries and by `event-stream` (when it lands) for message framing. It stays foreign-compatible: it works on its own without any other crate in the family.

<br>

## Contributing

Before opening a PR, the full local checklist must pass:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
cargo deny check
cargo audit
```

Any change touching the wire format requires a `proptest` round-trip and a determinism test in the same commit, and must keep `tests/golden.rs` (vectors written by pack-io 1.0.1) and `tests/golden_v2.rs` passing. Wire-format-breaking changes are only accepted for a new major version, with a migration note in [`CHANGELOG.md`](./CHANGELOG.md).

<br>

<hr>
<br>

<!-- LICENSE
############################################# -->
<div id="license">
    <h2>License</h2>
    <p>Licensed under either of</p>
    <ul>
        <li><b>Apache License, Version 2.0</b> — see <a href="./LICENSE-APACHE">LICENSE-APACHE</a> (<a href="http://www.apache.org/licenses/LICENSE-2.0" target="_blank">http://www.apache.org/licenses/LICENSE-2.0</a>)</li>
        <li><b>MIT License</b> — see <a href="./LICENSE-MIT">LICENSE-MIT</a> (<a href="http://opensource.org/licenses/MIT" target="_blank">http://opensource.org/licenses/MIT</a>)</li>
    </ul>
    <p>at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.</p>
</div>

<!-- FOOT COPYRIGHT
################################################# -->
<div align="center">
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong></sup>
</div>
