<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <b>pack-io-derive</b>
    <br>
    <sub>
        <sup>PROCEDURAL MACROS FOR pack-io</sup>
    </sub>
</h1>

<div align="center">
    <a href="https://crates.io/crates/pack-io-derive"><img alt="Crates.io" src="https://img.shields.io/crates/v/pack-io-derive"></a>
    <a href="https://crates.io/crates/pack-io-derive" alt="Download pack-io-derive"><img alt="Crates.io Downloads" src="https://img.shields.io/crates/d/pack-io-derive?color=%230099ff"></a>
    <a href="https://docs.rs/pack-io-derive" title="pack-io-derive Documentation"><img alt="docs.rs" src="https://img.shields.io/docsrs/pack-io-derive"></a>
    <a href="https://github.com/jamesgober/pack-io/actions"><img alt="GitHub CI" src="https://github.com/jamesgober/pack-io/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/rust-lang/rfcs/blob/master/text/2495-min-rust-version.md" title="MSRV"><img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.85%2B-blue"></a>
</div>

<br>

<div align="left">
    <p>
        Procedural macros for <a href="https://crates.io/crates/pack-io"><code>pack-io</code></a>. Provides <code>#[derive(Serialize)]</code>, <code>#[derive(Deserialize)]</code>, and <code>#[derive(DeserializeView)]</code> for any struct (named, tuple, unit) and any enum.
    </p>
    <p>
        <strong>This crate is not intended to be used directly.</strong> Add <code>pack-io</code> to your <code>Cargo.toml</code> with the <code>derive</code> feature enabled instead — the macros are re-exported at <code>pack_io::{Serialize, Deserialize, DeserializeView}</code> and pulled in automatically.
    </p>
</div>

<hr>
<br>

## Installation

Don't depend on `pack-io-derive` directly. Depend on `pack-io` with the `derive` feature:

```toml
[dependencies]
pack-io = { version = "0.4", features = ["derive"] }
```

## What the macros do

Three derives, all working on structs (named-field, tuple, unit) and enums (any variant shape), generic over type parameters:

```rust
use pack_io::{Serialize, Deserialize, DeserializeView};

#[derive(Serialize, Deserialize)]
struct Account {
    id: u64,
    handle: String,
    active: bool,
}

#[derive(Serialize, Deserialize)]
enum Event {
    Heartbeat,
    Login { user: u64, ip: String },
    Error(u32, String),
}

#[derive(DeserializeView)]
struct AccountView<'a> {
    id: u64,
    handle: &'a str,   // borrows directly from the input buffer
    active: bool,
}
```

| Derive               | Implements                       | Notes                                                                     |
|----------------------|----------------------------------|---------------------------------------------------------------------------|
| `Serialize`          | `pack_io::Serialize`             | Generic over the `pack_io::Encode` behaviour trait.                       |
| `Deserialize`        | `pack_io::Deserialize`           | Generic over the `pack_io::Decode` behaviour trait. Owning decode.        |
| `DeserializeView`    | `pack_io::DeserializeView<'a>`   | Zero-copy. Struct must have exactly one lifetime parameter.               |

Generated code is generic over `Encode` / `Decode`, so the same impl drives both the in-memory codec and the streaming `IoEncoder<W>` / `IoDecoder<R>` in the parent crate.

Field order in the source code is the encoded byte order. For enums, a `varint(variant_index)` prefix is emitted first, in source-declaration order starting at `0`. **Append new variants to the end** of an enum declaration to keep the wire shape backward-compatible — inserting in the middle shifts every later variant's index.

See the full [wire-format specification](https://github.com/jamesgober/pack-io/blob/main/docs/WIRE_FORMAT.md) and the [API reference](https://github.com/jamesgober/pack-io/blob/main/docs/API.md) in the parent repository.

## Compatibility

- **MSRV**: Rust 1.85 (2024 edition).
- **Version pinning**: `pack-io` depends on `pack-io-derive` with an exact `=X.Y.Z` constraint so mismatched derive output cannot leak across `pack-io` revisions.

<hr>
<br>

<!-- LICENSE
############################################# -->
<div id="license">
    <h2>License</h2>
    <p>Licensed under either of</p>
    <ul>
        <li><b>Apache License, Version 2.0</b> — see <a href="../LICENSE-APACHE">LICENSE-APACHE</a> (<a href="http://www.apache.org/licenses/LICENSE-2.0" target="_blank">http://www.apache.org/licenses/LICENSE-2.0</a>)</li>
        <li><b>MIT License</b> — see <a href="../LICENSE-MIT">LICENSE-MIT</a> (<a href="http://opensource.org/licenses/MIT" target="_blank">http://opensource.org/licenses/MIT</a>)</li>
    </ul>
    <p>at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.</p>
</div>

<!-- FOOT COPYRIGHT
################################################# -->
<div align="center">
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong></sup>
</div>
