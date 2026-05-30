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
        <strong>pack-io</strong> is a <b>compact binary wire format</b> for Rust built around three things existing serialization crates split into three separate libraries: <b>speed</b>, <b>schema evolution</b>, and <b>zero-copy deserialization</b>. It is engineered as the serialization substrate underneath <a href="https://github.com/jamesgober/network-protocol"><code>network-protocol</code></a>, <a href="https://github.com/jamesgober/wire-codec"><code>wire-codec</code></a>, and Hive DB - so it has to be fast, deterministic, schema-aware, and safe under untrusted input from day one.
    </p>
    <p>
        Existing crates each cover part of the problem. <code>bincode</code> is fast but has no schema evolution. <code>rkyv</code> gives zero-copy but requires unsafe alignment discipline at the use site. <code>postcard</code> is embedded-focused and lean. None of them own all three properties as a single coherent contract. <code>pack-io</code> does, with a small, predictable wire format you can read with a spec instead of with the source code.
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
        <strong>Status: pre-1.0, in active development.</strong> The wire format is being designed and frozen across the 0.x series; <code>1.0.0</code> will be the wire-format freeze. See <a href="./CHANGELOG.md"><code>CHANGELOG.md</code></a> for detail.
    </blockquote>
</div>


<hr>
<br>

<h2>What it does</h2>

- **Compact binary encoding** of Rust values into a small, predictable wire format
- **Schema-versioned messages** - producer and consumer can be at different revisions and still interoperate
- **Zero-copy deserialization** for `&[u8]` / `&str` / lengths-prefixed slices when the input lives long enough
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

## Installation

```toml
[dependencies]
pack-io = "0.1"

# With derive macro:
pack-io = { version = "0.1", features = ["derive"] }

# no_std build:
pack-io = { version = "0.1", default-features = false }
```

<br>

## Quick Start

```rust
use pack_io::{encode, decode};

#[derive(pack_io::Serialize, pack_io::Deserialize)]
struct Message {
    id: u64,
    text: String,
}

let msg = Message { id: 1, text: "hello".into() };

// The 80% case — one line each direction.
let bytes: Vec<u8> = encode(&msg)?;
let back: Message  = decode(&bytes)?;
```

That is the whole common case. No builder, no type parameters, no setup.

<br>

## Zero-Copy Decode

When the input lives long enough, decode without copying:

```rust
use pack_io::{decode_view, View};

let view: View<Message> = decode_view(&bytes)?;
let text: &str = view.text();  // borrows from `bytes`
```

<br>

## Schema Evolution

Add fields without breaking old readers. Old fields can be deprecated. Version negotiation is built in:

```rust
#[derive(pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 2)]
struct Message {
    id: u64,
    text: String,
    #[pack_io(since = 2)]
    timestamp: Option<u64>,   // new in v2; old encoders skip
}
```

<br>

## Testing

```bash
cargo test --all-features
RUSTFLAGS="--cfg loom" cargo test --test loom_decode
cargo bench --bench codec_bench
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```
<hr>
<br>

## Cross-Platform Support

**Tier 1 Support:**
- ✅ Linux (x86_64, aarch64)
- ✅ macOS (x86_64, Apple Silicon)
- ✅ Windows (x86_64)

Encoding is byte-deterministic across all three; the CI matrix runs every target on stable and MSRV.

<br>

## Where It Fits

`pack-io` is the serialization substrate under `network-protocol`, `wire-codec`, and Hive DB. It is consumed by `raft-io` for log entries and by `event-stream` (when it lands) for message framing. It stays foreign-compatible: it works on its own without any other crate in the family.

<br>

## Contributing

Before opening a PR, `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` must be clean. Any change touching the wire format requires a `proptest` round-trip and a determinism test.

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
