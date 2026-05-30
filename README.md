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
        <strong>pack-io</strong> is a <b>compact binary wire format</b> for Rust built around three properties existing serialization crates split across three separate libraries: <b>speed</b>, <b>schema evolution</b>, and <b>zero-copy deserialization</b>. It is engineered as the serialization substrate underneath <a href="https://github.com/jamesgober/network-protocol"><code>network-protocol</code></a>, <a href="https://github.com/jamesgober/wire-codec"><code>wire-codec</code></a>, and Hive DB - so it has to be fast, deterministic, schema-aware, and safe under untrusted input from day one.
    </p>
    <p>
        Existing crates each cover part of the problem. <code>bincode</code> is fast but has no schema evolution. <code>rkyv</code> gives zero-copy but requires unsafe alignment discipline at the use site. <code>postcard</code> is embedded-focused and lean. None of them own all three properties as a single coherent contract. <code>pack-io</code> does, behind a small, predictable wire format you can read with a spec instead of with the source code.
    </p>
    <p>
        The common-case API is one line - <code>encode(&amp;value)</code> and <code>decode::&lt;T&gt;(&amp;bytes)</code> - and that path is the fast path. Schema versions, evolution helpers, and the zero-copy view API live behind feature flags so the default build stays small.
    </p>
    <br>
    <hr>
    <p>
        <strong>MSRV is 1.85+</strong> (Rust 2024 edition). <code>no_std</code>-capable. Deterministic encoding. No <code>unsafe</code> on the safe-decoding path.
    </p>
    <blockquote>
        <strong>Status: pre-1.0, in active development.</strong> v0.1.0 is the scaffold release - structure, tooling, and quality gates only; codec logic lands across the 0.x series. The wire format is being designed and frozen across the 0.x line; <code>1.0.0</code> is the wire-format freeze. See <a href="./CHANGELOG.md"><code>CHANGELOG.md</code></a> for detail.
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

The 1.0 contract is the same wire format on every supported platform, the same bytes for the same value every time, and no panic / no unbounded allocation on any input.

<br>
<hr>
<br>

## What it does

- **Compact binary encoding** of Rust values into a small, predictable wire format
- **Schema-versioned messages** - producer and consumer can be at different revisions and still interoperate
- **Zero-copy deserialization** for `&[u8]` / `&str` / length-prefixed slices when the input lives long enough
- **Deterministic output** - the same value always produces the same bytes (canonical encoding)
- **Safe under untrusted input** - bounded allocation, length-prefix validation, no panics on malformed bytes
- **Runtime-agnostic** - synchronous codec, usable from any context

<br>

## Features

- **Compact** — small, fixed-overhead encoding; varint integers; length-prefixed byte slices
- **Schema evolution** — additive field changes, optional fields, version negotiation
- **Zero-copy decode** — view types that borrow from the input buffer where possible
- **Deterministic** — canonical encoding for hashing, signing, content-addressing
- **Safe defaults** — bounded allocation, validated lengths, no panics on bad input
- **`no_std`-capable** — embedded and constrained environments
- **Derive macro** — `#[derive(Serialize, Deserialize)]` for any struct or enum (feature `derive`)
- **Optional `serde` interop** — read and write `serde` types via a thin adapter (feature `serde`)

<hr>
<br>

## Roadmap snapshot

| Version | Scope | Status |
|---------|-------|:------:|
| `0.1.0` | Scaffold: structure, CI, lints, quality gates | ✅ shipped |
| `0.2.0` | Foundation: `encode` / `decode`, primitive types, `Serialize` / `Deserialize`, round-trip + determinism + adversarial-decode proptests | ✅ shipped |
| `0.3.0` | Wire-format freeze, collections, streaming over `Read` / `Write`, `docs/WIRE_FORMAT.md` | _next_ |
| `0.4.0` | `View<T>` zero-copy decode + `derive` macro | planned |
| `0.5.0` | Schema evolution attributes + version negotiation | planned |
| `0.6.0` | Optimization pass + comparative benchmarks | planned |
| `0.7.0` | Hardening, fuzz, API freeze | planned |
| `0.8.x` → `0.9.x` | Alpha → Beta → RC | planned |
| `1.0.0` | Wire-format + API freeze | planned |

The roadmap is followed strictly; phases are not skipped. Per-phase exit criteria are tracked internally and surfaced in each release note.

<hr>
<br>

## Installation

```toml
[dependencies]
pack-io = "0.2"

# With derive macro (planned for 0.4+):
pack-io = { version = "0.2", features = ["derive"] }

# no_std build:
pack-io = { version = "0.2", default-features = false }
```

<br>

## API surface (v0.2.0)

The foundation surface — Tier-1 free functions, the Tier-2 in-memory `Encoder` / `Decoder`, and the Tier-3 `Serialize` / `Deserialize` traits — is live. See [`docs/API.md`](./docs/API.md) for the full reference.

### Tier 1 — the lazy path

The headline. One function each direction, no setup, no type parameters the caller has to name beyond the target type.

```rust
use pack_io::{encode, decode};

let bytes: Vec<u8> = encode(&(7_u64, true, String::from("hello"))).unwrap();
let back: (u64, bool, String) = decode(&bytes).unwrap();
assert_eq!(back, (7, true, String::from("hello")));
```

### Tier 2 — the in-memory `Encoder` / `Decoder`

Re-use a single `Vec<u8>` across many encodes; read several values from one buffer. Configuration (`Config::max_alloc`) is validated at construction time, not on every operation. Streaming over `std::io::Read` / `Write` arrives in `0.3`.

```rust
use pack_io::{Encoder, Decoder, Config};

let mut enc = Encoder::new();
enc.write(&7_u64).unwrap();
enc.write(&"hello").unwrap();
let bytes = enc.into_inner();

let cfg = Config::new().with_max_alloc(16 * 1024); // refuse > 16 KiB allocs
let mut dec = Decoder::with_config(&bytes, cfg).unwrap();
let n: u64 = dec.read().unwrap();
let s: String = dec.read().unwrap();
assert_eq!((n, s.as_str()), (7, "hello"));
```

### Tier 3 — implement [`Serialize`] / [`Deserialize`] on your own types

Today: write the impls by hand. The derive macro arrives in `0.4`.

```rust
use pack_io::{Serialize, Deserialize, Encoder, Decoder, SerialError};

struct Point { x: i32, y: i32 }

impl Serialize for Point {
    fn serialize(&self, enc: &mut Encoder) -> Result<(), SerialError> {
        self.x.serialize(enc)?;
        self.y.serialize(enc)
    }
}

impl Deserialize for Point {
    fn deserialize(dec: &mut Decoder<'_>) -> Result<Self, SerialError> {
        Ok(Point {
            x: i32::deserialize(dec)?,
            y: i32::deserialize(dec)?,
        })
    }
}
```

### Primitive types supported in v0.2.0

`u8` … `u128`, `i8` … `i128`, `usize` / `isize`, `bool`, `f32`, `f64`, `String` / `&str`, `Vec<u8>` / `&[u8]`, fixed-size arrays `[T; N]`, tuples of arity 1 … 12, `Option<T>`, `Result<T, E>`, and `()`.

Collections (`Vec<T>`, `HashMap`, …), schema evolution, and the zero-copy `View<T>` surface arrive in later 0.x phases.

<br>

## Invariants (held from v0.1.0)

- **Round-trip integrity** — `decode(encode(v)) == v` for every supported type, under any input.
- **Determinism** — the same value always produces the same bytes; no map-iteration-order leaks, no time-dependence, no platform-dependence.
- **Safe decode** — no panic, no unbounded allocation, no read past input, on any byte sequence.
- **Wire-format stability** — frozen at `1.0`; any `1.x` decoder reads any `1.x`-or-earlier encoding.

These invariants hold for every release in the `0.x` series. As of `0.2.0` they are enforced by `proptest` round-trip + determinism harnesses for every primitive (46 properties) and an adversarial-decode harness that fuzzes the public decode surface with random bytes, truncations, and hostile length prefixes (20 properties). A `cargo-fuzz` harness lands in `0.7`.

<br>

## Testing

```bash
# Stable + MSRV (1.85) on Linux / macOS / Windows, full feature matrix
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo test --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

# Supply chain
cargo audit
cargo deny check

# Concurrency model checking (decoders are stateless; loom coverage is light-touch)
RUSTFLAGS="--cfg loom" cargo test --test loom_codec

# Microbenchmarks
cargo bench --bench codec_bench
```

<br>

## Examples

Each example is self-contained and runs against the published API of the version it was added in.

```bash
cargo run --example basic_roundtrip --release   # Tier-1 encode/decode of a tuple
cargo run --example primitive_tour --release    # one encoded value per primitive type
cargo run --example reuse_buffer --release      # Tier-2 Encoder + multi-value Decoder
```

<hr>
<br>

## Cross-Platform Support

**Tier 1 Support:**
- ✅ Linux (x86_64, aarch64)
- ✅ macOS (x86_64, Apple Silicon)
- ✅ Windows (x86_64)

Encoding is byte-deterministic across all three; the CI matrix runs every target on stable and MSRV. Platform-specific behaviour is forbidden in the codec — there is no `#[cfg(target_os = …)]` branch on the encode or decode path.

<br>

## Where It Fits

`pack-io` is the serialization substrate under [`network-protocol`](https://github.com/jamesgober/network-protocol), [`wire-codec`](https://github.com/jamesgober/wire-codec), and Hive DB. It is consumed by [`raft-io`](https://github.com/jamesgober/raft-io) for log entries and by `event-stream` (when it lands) for message framing. It stays foreign-compatible: it works on its own without any other crate in the family.

<br>

## Contributing

Before opening a PR, the full local checklist must pass:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo deny check
cargo audit
```

Any change touching the wire format requires a `proptest` round-trip and a determinism test in the same commit. Wire-format-breaking changes are not accepted after `0.3` without an accompanying migration note in [`CHANGELOG.md`](./CHANGELOG.md).

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
