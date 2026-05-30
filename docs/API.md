<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>pack-io</b><br>
    <sub><sup>API REFERENCE</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="../CHANGELOG.md" title="Changelog"><b>CHANGELOG</b></a>
        <span>&nbsp;│&nbsp;</span>
        <span>API</span>
        <span>&nbsp;│&nbsp;</span>
        <a href="./release/" title="Release Notes"><b>RELEASES</b></a>
    </sup>
</div>
<br>

> Reference for every public item in `pack-io`, with runnable examples.
>
> **Status: pre-1.0.** This document tracks the API surface as it lands across
> the `0.x` series. Sections marked _(planned: vX.Y)_ describe the intended
> surface and are filled in as each roadmap phase ships. Items not marked
> _planned_ are present in the version listed in [`Cargo.toml`](../Cargo.toml)
> and exercised by the test suite.

---

## Table of Contents

- [Installation](#installation)
- [Overview](#overview)
- [Current public surface (v0.1.0)](#current-public-surface-v010)
  - [`VERSION`](#version)
- [Tier 1 — the lazy path](#tier-1--the-lazy-path)
  - [`encode`](#encode) _(planned: 0.2)_
  - [`decode`](#decode) _(planned: 0.2)_
- [Tier 2 — the configured path](#tier-2--the-configured-path)
  - [`Encoder` / `Decoder`](#encoder--decoder) _(planned: 0.3)_
- [Tier 3 — the power path](#tier-3--the-power-path)
  - [`Serialize` / `Deserialize` traits](#serialize--deserialize-traits) _(planned: 0.2)_
  - [`View<T>` zero-copy decode](#view-zero-copy-decode) _(planned: 0.4)_
- [Schema evolution](#schema-evolution) _(planned: 0.5)_
- [Wire format](#wire-format) _(spec lands at 0.3)_
- [Errors](#errors) _(planned: 0.2)_
- [Feature flags](#feature-flags)
- [Cross-platform notes](#cross-platform-notes)
- [Compatibility & semver](#compatibility--semver)

---

## Installation

```toml
[dependencies]
pack-io = "0.1"
```

With the derive macro (planned for `0.4`):

```toml
[dependencies]
pack-io = { version = "0.1", features = ["derive"] }
```

`no_std` build:

```toml
[dependencies]
pack-io = { version = "0.1", default-features = false }
```

MSRV is **Rust 1.85** (2024 edition). The CI matrix runs every supported
platform on both stable and MSRV; downstream crates may rely on the declared
`rust-version` in [`Cargo.toml`](../Cargo.toml) for resolver-aware version
selection.

---

## Overview

`pack-io` exposes a compact binary codec. The common case is a function call;
advanced use is an encoder / decoder for streaming; the full surface is the
`Serialize` / `Deserialize` traits and the zero-copy `View<T>` types.

- The hot path never allocates beyond the output buffer.
- The encoding is **deterministic** — the same value always produces the same
  bytes, regardless of platform, insertion order, or build flags.
- Decoding is **safe under untrusted input** by default — bounded allocation,
  validated length prefixes, no panics, no reads past the input.

These properties hold across every release in the `0.x` series and are
enforced by the test suite that lands alongside the codec in `0.2`.

---

## Current public surface (v0.1.0)

The `0.1.0` scaffold release exposes one symbol: a compile-time version
constant. The remainder of this document describes the **target** surface as
it lands across the `0.x` series.

### `VERSION`

```rust
pub const VERSION: &str;
```

The semantic version of the crate, exposed at compile time. Mirrors
`Cargo.toml` exactly via the `CARGO_PKG_VERSION` environment variable —
no parsing, no allocation.

**Use it when** a downstream needs to log, report, or assert the codec
version it was compiled against without parsing the manifest at runtime
(useful when a binary is shipped as a single file with no `Cargo.toml`).

**Example — print the codec version:**

```rust
println!("pack-io {}", pack_io::VERSION);
```

**Example — assert at build time that the major version has not jumped:**

```rust
const _: () = {
    // pack-io stays in the 0.x line until the wire format freezes at 1.0.
    let v = pack_io::VERSION.as_bytes();
    assert!(!v.is_empty() && v[0] == b'0');
};
```

**Example — guard a feature path on the codec version reported at runtime:**

```rust
fn supports_zero_copy_view() -> bool {
    // The View<T> surface lands in 0.4. Compare lexically while we are in 0.x;
    // switch to a real semver-compare crate once the codec leaves 0.x.
    pack_io::VERSION >= "0.4.0"
}
```

---

## Tier 1 — the lazy path

The one-line surface for the ~80 % case. Two free functions, no builder, no
type parameters the caller has to name beyond the target type.

### `encode`

_Planned: v0.2.0._ Encode a value into a freshly allocated `Vec<u8>`.

```rust,ignore
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, SerialError>;
```

**Parameters:**

| Name    | Type      | Description                              |
|---------|-----------|------------------------------------------|
| `value` | `&T`      | The value to encode. Borrowed, never cloned. |

**Returns:** `Ok(Vec<u8>)` — the encoded bytes; the buffer is sized exactly
to fit the value (no spare capacity). `Err(SerialError)` for any I/O or
encoding failure (e.g. a downstream `std::io::Write` impl that errors out
when wired through the streaming `Encoder`).

**Example — round-trip a struct:**

```rust,ignore
use pack_io::{encode, decode};

#[derive(pack_io::Serialize, pack_io::Deserialize, PartialEq, Debug)]
struct Message { id: u64, text: String }

let msg = Message { id: 7, text: "hello".into() };
let bytes = encode(&msg)?;
let back: Message = decode(&bytes)?;
assert_eq!(msg, back);
# Ok::<_, pack_io::SerialError>(())
```

**Example — encode a primitive:**

```rust,ignore
use pack_io::encode;

let bytes = encode(&42_u64)?;
assert_eq!(bytes.len(), 1); // varint: 42 fits in one byte
# Ok::<_, pack_io::SerialError>(())
```

### `decode`

_Planned: v0.2.0._ Decode a value from a byte slice.

```rust,ignore
pub fn decode<T: Deserialize>(bytes: &[u8]) -> Result<T, SerialError>;
```

**Parameters:**

| Name    | Type      | Description                              |
|---------|-----------|------------------------------------------|
| `bytes` | `&[u8]`   | The encoded input. Read-only, validated as it is consumed. |

**Returns:** `Ok(T)` on success. `Err(SerialError)` for any malformed input
(short read, oversized length prefix, invalid variant tag, …). Never panics,
never allocates unboundedly, never reads past `bytes`.

**Example — accept untrusted input safely:**

```rust,ignore
use pack_io::decode;

let bytes: &[u8] = network_buffer();
match decode::<Message>(bytes) {
    Ok(msg) => handle(msg),
    Err(err) => log::warn!("rejected malformed payload: {err}"),
}
```

---

## Tier 2 — the configured path

### `Encoder` / `Decoder`

_Planned: v0.3.0._ Streaming encoder and decoder for callers that want to
write into an existing buffer (avoiding the per-call allocation of Tier 1) or
read from anything that implements `std::io::Read`.

```rust,ignore
pub struct Encoder<W: Write> { /* … */ }
impl<W: Write> Encoder<W> {
    pub fn new(out: W) -> Self;
    pub fn write<T: Serialize>(&mut self, value: &T) -> Result<(), SerialError>;
    pub fn into_inner(self) -> W;
}

pub struct Decoder<R: Read> { /* … */ }
impl<R: Read> Decoder<R> {
    pub fn new(input: R) -> Self;
    pub fn read<T: Deserialize>(&mut self) -> Result<T, SerialError>;
}
```

**Example — write several values into one buffer:**

```rust,ignore
use pack_io::Encoder;

let mut buf = Vec::with_capacity(1024);
let mut enc = Encoder::new(&mut buf);
for msg in messages.iter() {
    enc.write(msg)?;
}
let written: usize = buf.len();
# Ok::<_, pack_io::SerialError>(())
```

**Example — stream from a file:**

```rust,ignore
use pack_io::Decoder;
use std::{fs::File, io::BufReader};

let file = BufReader::new(File::open("log.pack")?);
let mut dec = Decoder::new(file);
while let Ok(msg) = dec.read::<Message>() {
    handle(msg);
}
# Ok::<_, Box<dyn std::error::Error>>(())
```

---

## Tier 3 — the power path

### `Serialize` / `Deserialize` traits

_Planned: v0.2.0._ The trait surface every concrete type implements. The
derive macro (feature `derive`, lands in `0.4`) writes a sound implementation
for any struct or enum.

```rust,ignore
pub trait Serialize {
    fn serialize<W: Write>(&self, w: &mut Encoder<W>) -> Result<(), SerialError>;
}

pub trait Deserialize: Sized {
    fn deserialize<R: Read>(r: &mut Decoder<R>) -> Result<Self, SerialError>;
}
```

**Example — hand-written impls (rarely needed once the derive lands):**

```rust,ignore
use pack_io::{Serialize, Deserialize, Encoder, Decoder, SerialError};
use std::io::{Read, Write};

struct Point { x: i32, y: i32 }

impl Serialize for Point {
    fn serialize<W: Write>(&self, w: &mut Encoder<W>) -> Result<(), SerialError> {
        self.x.serialize(w)?;
        self.y.serialize(w)
    }
}

impl Deserialize for Point {
    fn deserialize<R: Read>(r: &mut Decoder<R>) -> Result<Self, SerialError> {
        Ok(Point {
            x: i32::deserialize(r)?,
            y: i32::deserialize(r)?,
        })
    }
}
```

### `View<T>` zero-copy decode

_Planned: v0.4.0._ Zero-copy view types that borrow directly from the input
buffer. For borrow-heavy payloads this is measurably faster than the owning
`decode` because no `String` / `Vec<u8>` is materialised.

```rust,ignore
pub struct View<'a, T> { /* borrows from &'a [u8] */ }

pub fn decode_view<'a, T>(bytes: &'a [u8]) -> Result<View<'a, T>, SerialError>;
```

**Example — borrow string fields without copying:**

```rust,ignore
use pack_io::{decode_view, View};

let view: View<Message> = decode_view(&bytes)?;
let text: &str = view.text();           // borrows from `bytes`
let id: u64 = view.id();
# Ok::<_, pack_io::SerialError>(())
```

The borrow checker prevents the view outliving the buffer; no `unsafe` is
required at the call site, in contrast to alignment-sensitive zero-copy
crates.

---

## Schema evolution

_Planned: v0.5.0._ Producers and consumers at different revisions of a type
remain interoperable as long as the changes are additive.

**Annotation surface (target):**

| Attribute | Where | Meaning |
|-----------|-------|---------|
| `#[pack_io(version = N)]` | struct / enum | The current schema revision. Encoded in the payload header. |
| `#[pack_io(since = N)]` | field | Field was added in version `N`. Older readers skip it; older writers emit no bytes for it. |
| `#[pack_io(deprecated = N)]` | field | Field was removed in version `N`. Newer readers fill defaults. |

**Example — additive field:**

```rust,ignore
#[derive(pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 2)]
struct Message {
    id: u64,
    text: String,
    #[pack_io(since = 2)]
    timestamp: Option<u64>, // present in v2; absent in v1 payloads
}
```

**Example — peek at the version before deserialising:**

```rust,ignore
use pack_io::peek_version;

let version: u32 = peek_version(&bytes)?;
match version {
    1 => handle_v1(decode::<MessageV1>(&bytes)?),
    2 => handle_v2(decode::<MessageV2>(&bytes)?),
    other => log::warn!("unsupported schema version: {other}"),
}
# Ok::<_, pack_io::SerialError>(())
```

---

## Wire format

_The normative spec lands as [`docs/WIRE_FORMAT.md`](./WIRE_FORMAT.md) at
v0.3.0 when the format freezes._ It is written so a reader can implement a
compatible codec without reading the source.

Until that document ships, the encoding is considered **unstable** across
the `0.x` series. Wire-format-breaking changes are called out prominently in
[`CHANGELOG.md`](../CHANGELOG.md). From `0.3` onward, breaking the format
requires a documented migration in the same commit.

---

## Errors

_Planned: v0.2.0._ A single `#[non_exhaustive]` error enum built on
[`error-forge`](https://github.com/jamesgober/error-forge) covers both
construction- and decode-time failures.

```rust,ignore
#[non_exhaustive]
pub enum SerialError {
    /// Input ended mid-value.
    UnexpectedEof,
    /// A length prefix exceeded the remaining buffer (or a configured cap).
    InvalidLength { declared: usize, remaining: usize },
    /// A varint exceeded the maximum legal byte count for its target type.
    VarintOverflow,
    /// An enum variant tag did not match any known variant.
    UnknownVariant { tag: u32 },
    /// An I/O error from the underlying `Read` / `Write` (Tier 2 only).
    Io(std::io::Error),
}
```

The encode path is infallible for sized in-memory values (the Tier-1
`encode` returns `Result<Vec<u8>, SerialError>` only because the Tier-2
streaming path can wrap it).

**Per-variant guidance (target):**

- `UnexpectedEof`, `InvalidLength`, `VarintOverflow`, `UnknownVariant` —
  treat as untrusted-input failures. Log at `warn`, drop the message, do
  not retry blindly.
- `Io` — same handling as any other transport `std::io::Error`.

---

## Feature flags

| Feature  | Default | Description |
|----------|---------|-------------|
| `std`    | yes     | Standard library. Off → `no_std`. |
| `derive` | no      | `#[derive(Serialize, Deserialize)]` proc-macros. _(populated at 0.4)_ |
| `schema` | no      | Schema-versioning and evolution helpers. _(populated at 0.5)_ |
| `serde`  | no      | Optional `serde` interop shims. |

All feature flags are **additive**. Enabling a feature never removes or
changes existing behaviour; disabling a feature never breaks code that did
not opt into it.

---

## Cross-platform notes

- Tier-1 supported targets: Linux (`x86_64`, `aarch64`), macOS (`x86_64`,
  Apple Silicon), Windows (`x86_64`). All three run the full CI matrix on
  every commit, on both stable and MSRV.
- Encoding is byte-deterministic across all three. There is no
  `#[cfg(target_os = …)]` branch on the encode or decode path.
- `no_std` builds rely on `core` + `alloc` only — no `std::io`, no thread
  locals, no `Instant`. The Tier-2 `Encoder` / `Decoder` are gated on `std`.

---

## Compatibility & semver

- Pre-1.0: breaking changes bump MINOR (per the project versioning
  strategy). They are called out under their own subheading in the
  changelog.
- Post-1.0: SemVer in the strict sense. Breaking changes bump MAJOR; the
  wire format never breaks within a MAJOR.
- Deprecated items remain available for at least one MAJOR after the
  `#[deprecated]` attribute is added.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
