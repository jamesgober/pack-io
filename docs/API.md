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
        <a href="./WIRE_FORMAT.md" title="Wire Format Spec"><b>WIRE FORMAT</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./release/" title="Release Notes"><b>RELEASES</b></a>
    </sup>
</div>
<br>

> Reference for every public item in `pack-io`, with runnable examples.
>
> **Status: API frozen as of v0.7.0. Wire format frozen at v0.3.0
> (currently spec version 1.2).** Every public type, trait, free function,
> constant, attribute, and feature flag listed below is part of the
> frozen surface that ships in v1.0. Bug fixes, performance work, and
> *backwards-compatible* additions (new derives for new field types, new
> hardening passes) may land in v0.7.x → v0.9.x. Any source-breaking or
> wire-format-breaking change is deferred to v2.0.

---

## Table of Contents

- [Installation](#installation)
- [Frozen public surface](#frozen-public-surface)
- [Overview](#overview)
- [Tier 1 — the lazy path](#tier-1--the-lazy-path)
  - [`encode`](#encode)
  - [`decode`](#decode)
- [Tier 2a — in-memory codec](#tier-2a--in-memory-codec)
  - [`Encoder`](#encoder)
  - [`Decoder`](#decoder)
  - [`Config`](#config)
- [Tier 2b — streaming codec](#tier-2b--streaming-codec)
  - [`IoEncoder<W>`](#ioencoder)
  - [`IoDecoder<R>`](#iodecoder)
  - [`encode_into`](#encode_into)
  - [`decode_from`](#decode_from)
- [Tier 3 — the trait surface](#tier-3--the-trait-surface)
  - [`Encode` / `Decode` (behaviour traits)](#encode--decode-behaviour-traits)
  - [`Serialize` / `Deserialize` (value traits)](#serialize--deserialize-value-traits)
  - [`#[derive(Serialize, Deserialize)]`](#derive-serialize-deserialize)
- [Zero-copy decode](#zero-copy-decode)
  - [`DeserializeView<'a>`](#deserializeview)
  - [`decode_view`](#decode_view)
  - [`Decoder::read_length_prefixed_borrowed`](#decoder-read-length-prefixed-borrowed)
  - [`#[derive(DeserializeView)]`](#derive-deserializeview)
- [Schema evolution](#schema-evolution)
  - [`#[pack_io(version = N)]`](#pack_io-version)
  - [`#[pack_io(since = N)]`](#pack_io-since)
  - [`#[pack_io(deprecated = N)]`](#pack_io-deprecated)
  - [`peek_version`](#peek_version)
- [Errors](#errors)
  - [`SerialError`](#serialerror)
  - [`Result<T>`](#resultt)
- [Supported types](#supported-types)
- [Wire format](#wire-format)
- [Feature flags](#feature-flags)
- [Other public items](#other-public-items)
- [Cross-platform notes](#cross-platform-notes)
- [Compatibility & semver](#compatibility--semver)

---

## Installation

```toml
[dependencies]
pack-io = "0.7"
```

`no_std` build:

```toml
[dependencies]
pack-io = { version = "0.7", default-features = false }
```

MSRV is **Rust 1.85** (2024 edition). The CI matrix runs every supported
platform on both stable and MSRV; downstream crates may rely on the declared
`rust-version` in [`Cargo.toml`](../Cargo.toml) for resolver-aware version
selection.

---

## Frozen public surface

The exhaustive list below is the v1.0 contract. Anything not on this
list is an internal detail and may change at any time without a major
version bump.

### Types

| Path                              | Kind                | Frozen at |
|-----------------------------------|---------------------|-----------|
| `pack_io::Encoder`                | concrete struct     | v0.2.0    |
| `pack_io::Decoder<'a>`            | concrete struct     | v0.2.0    |
| `pack_io::Config`                 | `#[non_exhaustive]` struct | v0.2.0 |
| `pack_io::SerialError`            | `#[non_exhaustive]` enum   | v0.2.0 |
| `pack_io::Result<T>`              | type alias          | v0.2.0    |
| `pack_io::IoEncoder<W: Write>`    | concrete struct     | v0.3.0    |
| `pack_io::IoDecoder<R: Read>`     | concrete struct     | v0.3.0    |

### Traits

| Path                              | Methods | Frozen at |
|-----------------------------------|---------|-----------|
| `pack_io::Serialize`              | `serialize`, `serialize_slice` (default) | v0.6.0 |
| `pack_io::Deserialize`            | `deserialize`, `deserialize_many` (default) | v0.6.0 |
| `pack_io::DeserializeView<'a>`    | `deserialize_view` | v0.4.0 |
| `pack_io::Encode`                 | `write_byte`, `write_bytes`, `reserve`, `write_varint_u64`, `write_varint_u128` | v0.3.0 |
| `pack_io::Decode`                 | `read_byte`, `read_into`, `max_alloc`, `read_varint_u64`, `read_varint_u128`, `read_length_prefixed` | v0.3.0 |

### Free functions

| Path                              | Frozen at |
|-----------------------------------|-----------|
| `pack_io::encode`                 | v0.2.0    |
| `pack_io::decode`                 | v0.2.0    |
| `pack_io::decode_view`            | v0.4.0    |
| `pack_io::encode_into`            | v0.3.0    |
| `pack_io::decode_from`            | v0.3.0    |
| `pack_io::peek_version`           | v0.5.0    |

### Constants

| Path                              | Frozen at |
|-----------------------------------|-----------|
| `pack_io::VERSION`                | v0.1.0    |

### Inherent methods on `Encoder`

| Method                            | Frozen at |
|-----------------------------------|-----------|
| `Encoder::new`                    | v0.2.0    |
| `Encoder::with_capacity`          | v0.6.0    |
| `Encoder::into_buffer`            | v0.2.0    |
| `Encoder::as_bytes`               | v0.2.0    |
| `Encoder::into_inner`             | v0.2.0    |
| `Encoder::take`                   | v0.2.0    |
| `Encoder::write`                  | v0.2.0    |

### Inherent methods on `Decoder<'a>`

| Method                                       | Frozen at |
|----------------------------------------------|-----------|
| `Decoder::new`                               | v0.2.0    |
| `Decoder::with_config`                       | v0.2.0    |
| `Decoder::position`                          | v0.2.0    |
| `Decoder::remaining`                         | v0.2.0    |
| `Decoder::is_empty`                          | v0.2.0    |
| `Decoder::read`                              | v0.2.0    |
| `Decoder::read_length_prefixed_borrowed`     | v0.4.0    |

### Inherent methods on `IoEncoder<W>` / `IoDecoder<R>`

| Method                                       | Frozen at |
|----------------------------------------------|-----------|
| `IoEncoder::new`, `writer`, `writer_mut`, `into_inner`, `write` | v0.3.0 |
| `IoDecoder::new`, `with_config`, `reader`, `into_inner`, `read` | v0.3.0 |

### Inherent methods on `Config`

| Method                            | Frozen at |
|-----------------------------------|-----------|
| `Config::new` (`const fn`)        | v0.2.0    |
| `Config::with_max_alloc` (`const fn`) | v0.2.0 |

### Re-exported derive macros (feature `derive`)

| Path                              | Frozen at |
|-----------------------------------|-----------|
| `#[derive(pack_io::Serialize)]`   | v0.4.0    |
| `#[derive(pack_io::Deserialize)]` | v0.4.0    |
| `#[derive(pack_io::DeserializeView)]` | v0.4.0 |

### Schema attributes (feature `schema`, implies `derive`)

| Path                                       | Frozen at |
|--------------------------------------------|-----------|
| `#[pack_io(version = N)]` on types         | v0.5.0    |
| `#[pack_io(since = N)]` on fields          | v0.5.0    |
| `#[pack_io(deprecated = N)]` on fields     | v0.5.0    |

### `SerialError` variants

| Variant                                          | Frozen at |
|--------------------------------------------------|-----------|
| `UnexpectedEof { needed, remaining }`            | v0.2.0    |
| `InvalidLength { declared, remaining }`          | v0.2.0    |
| `VarintOverflow`                                 | v0.2.0    |
| `IntegerOutOfRange`                              | v0.2.0    |
| `InvalidBool { byte }`                           | v0.2.0    |
| `InvalidUtf8`                                    | v0.2.0    |
| `InvalidTag { kind, tag }`                       | v0.2.0    |
| `TrailingBytes { remaining }`                    | v0.2.0    |
| `UnknownVariant { kind, index }`                 | v0.4.0    |
| `Io { kind, message }` *(feature `std`)*         | v0.3.0    |

`SerialError` is `#[non_exhaustive]`, so callers must include a wildcard
`match` arm. New variants may be added in backwards-compatible MINOR
releases.

### Feature flags

| Feature  | Default | Frozen at |
|----------|---------|-----------|
| `std`    | yes     | v0.1.0    |
| `derive` | no      | v0.4.0    |
| `schema` | no      | v0.5.0    |
| `serde`  | no      | v0.1.0 (reserved; populated later) |

All feature flags are **additive**. Enabling a feature never removes or
changes existing behaviour; disabling a feature never breaks code that
did not opt into it.

### Out of the public surface (intentional)

- The `pack-io-derive` crate is an implementation detail. Depending on
  it directly is unsupported — the version pin is exact (`=X.Y.Z`)
  precisely to prevent users from getting a different revision than
  the parent crate expects.
- The `varint` module is `pub(crate)`. The wire-format spec at
  [`docs/WIRE_FORMAT.md`](./WIRE_FORMAT.md) defines the LEB128 layout
  normatively; consumers should not depend on any specific helper
  function signature.
- Test-only and benchmark-only items in `tests/` and `benches/` are
  not part of the public surface.

---

## Overview

`pack-io` exposes a compact binary codec in three layers:

- **Tier 1** — the [`encode`](#encode) / [`decode`](#decode) free functions.
  One line each direction, no setup, no type parameters the caller has to
  name beyond the target type.
- **Tier 2** — concrete encoder / decoder pairs:
  - **2a, in-memory**: [`Encoder`](#encoder) + [`Decoder`](#decoder),
    backed by `Vec<u8>` / `&[u8]`. Best when the whole payload already
    lives in memory.
  - **2b, streaming**: [`IoEncoder<W>`](#ioencoder) +
    [`IoDecoder<R>`](#iodecoder), wrapping any `std::io::Write` / `Read`.
    Plus [`encode_into`](#encode_into) / [`decode_from`](#decode_from)
    convenience helpers. Gated on the default `std` feature.
- **Tier 3** — the [`Serialize`](#serialize--deserialize-value-traits) and
  [`Deserialize`](#serialize--deserialize-value-traits) traits implemented
  on your own types. Both are generic over the
  [`Encode`](#encode--decode-behaviour-traits) and
  [`Decode`](#encode--decode-behaviour-traits) behaviour traits, so a
  single impl works through every concrete encoder / decoder the crate
  ships.

Properties that hold across every release:

- The encode hot path never allocates beyond the output buffer.
- Encoding is **deterministic** — the same value always produces the same
  bytes, regardless of platform, insertion order, hash randomisation, or
  build flags.
- Decoding is **safe under untrusted input** — bounded allocation,
  validated length prefixes, no panics, no reads past the input.

The normative byte-level spec lives in
[`docs/WIRE_FORMAT.md`](./WIRE_FORMAT.md); it is frozen for the `1.x` line
as of `v0.3.0`.

---

## Tier 1 — the lazy path

### `encode`

```rust,ignore
pub fn encode<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>>;
```

Encode `value` into a freshly allocated `Vec<u8>`.

**Example:**

```rust
let bytes = pack_io::encode(&42_u64).unwrap();
let back: u64 = pack_io::decode(&bytes).unwrap();
assert_eq!(back, 42);
```

**Example — heterogeneous tuple:**

```rust
let v = (1_u64, true, String::from("hello"));
let bytes = pack_io::encode(&v).unwrap();
let back: (u64, bool, String) = pack_io::decode(&bytes).unwrap();
assert_eq!(back, v);
```

### `decode`

```rust,ignore
pub fn decode<T: Deserialize>(bytes: &[u8]) -> Result<T>;
```

Decode a value of type `T` from `bytes`, requiring the input to be fully
consumed. Trailing input is rejected with
[`SerialError::TrailingBytes`](#serialerror).

**Example — strict decode rejects trailing bytes:**

```rust
let mut bytes = pack_io::encode(&7_u8).unwrap();
bytes.push(0xff);
let err = pack_io::decode::<u8>(&bytes).unwrap_err();
assert!(matches!(err, pack_io::SerialError::TrailingBytes { remaining: 1 }));
```

---

## Tier 2a — in-memory codec

### `Encoder`

In-memory encoder. Writes into an owned `Vec<u8>`; the buffer can be
swapped in / out so a single allocation serves many encodes.

| Constructor                         | Purpose                                          |
|-------------------------------------|--------------------------------------------------|
| `Encoder::new()`                    | Empty buffer.                                    |
| `Encoder::into_buffer(buf)`         | Append into the caller's `Vec<u8>` (no alloc).   |
| `Encoder::default()`                | Same as `new()`.                                 |

| Method                  | Returns       | Purpose                                  |
|-------------------------|---------------|------------------------------------------|
| `write(&value)`         | `Result<()>`  | Append `value`'s wire-format bytes.      |
| `as_bytes()`            | `&[u8]`       | Borrow the bytes written so far.         |
| `into_inner()`          | `Vec<u8>`     | Consume the encoder, return the buf.     |
| `take()`                | `Vec<u8>`     | Swap in an empty buffer, return the old. |

**Example — write three values into one buffer:**

```rust
use pack_io::Encoder;

let mut enc = Encoder::new();
enc.write(&7_u64).unwrap();
enc.write(&true).unwrap();
enc.write(&"hello").unwrap();
let bytes = enc.into_inner();
assert!(bytes.len() > 0);
```

### `Decoder`

Cursored decoder. Borrows from an input slice and advances a position
pointer as values are read. Bounds-checked on every operation.

| Constructor                              | Purpose                                              |
|------------------------------------------|------------------------------------------------------|
| `Decoder::new(bytes)`                    | Default config (`max_alloc = 1 GiB`).                |
| `Decoder::with_config(bytes, cfg)`       | Validates `cfg`; returns `Err` if `max_alloc == 0`.  |

| Method         | Returns          | Purpose                                  |
|----------------|------------------|------------------------------------------|
| `read::<T>()`  | `Result<T>`      | Decode one value of `T`, advance cursor. |
| `position()`   | `usize`          | Bytes consumed so far.                   |
| `remaining()`  | `usize`          | Bytes left in the input.                 |
| `is_empty()`   | `bool`           | `remaining() == 0`.                      |

**Example — read several values from one buffer:**

```rust
use pack_io::{Encoder, Decoder};

let mut enc = Encoder::new();
enc.write(&7_u64).unwrap();
enc.write(&true).unwrap();
let bytes = enc.into_inner();

let mut dec = Decoder::new(&bytes);
let n: u64 = dec.read().unwrap();
let b: bool = dec.read().unwrap();
assert!(dec.is_empty());
assert_eq!((n, b), (7, true));
```

### `Config`

```rust,ignore
#[non_exhaustive]
pub struct Config {
    pub max_alloc: usize,
}
```

`#[non_exhaustive]` so future MINOR releases can add knobs without
breaking downstream code.

| Constructor                          | Purpose                                            |
|--------------------------------------|----------------------------------------------------|
| `Config::new()`                      | Default — `max_alloc = 1 GiB`. `const fn`.         |
| `Config::default()`                  | Same as `new()`.                                   |
| `Config::with_max_alloc(n)`          | Replace `max_alloc`, return updated. `const fn`.   |

The `max_alloc` field caps the largest single length-prefixed value the
decoder will allocate. Length prefixes (and collection element counts)
that exceed it are rejected with [`SerialError::InvalidLength`](#serialerror)
**before** the decoder allocates a single byte.

**Example — tight cap for untrusted input:**

```rust
let cfg = pack_io::Config::new().with_max_alloc(16 * 1024);
assert_eq!(cfg.max_alloc, 16 * 1024);
```

---

## Tier 2b — streaming codec

The streaming pair (`std` feature, default-on) wraps any `std::io::Write`
or `Read`. Bytes are written through to the underlying sink as they are
produced — useful for sockets, files, and pipes where buffering the entire
payload in memory is undesirable. Both decoder flavours surface I/O
failure through [`SerialError::Io`](#serialerror).

### `IoEncoder`

```rust,ignore
pub struct IoEncoder<W: Write> { /* … */ }
```

| Constructor                    | Purpose                                          |
|--------------------------------|--------------------------------------------------|
| `IoEncoder::new(writer)`       | Wrap any `std::io::Write`.                       |

| Method               | Returns       | Purpose                                  |
|----------------------|---------------|------------------------------------------|
| `write(&value)`      | `Result<()>`  | Encode straight into the writer.         |
| `writer()`           | `&W`          | Borrow the underlying writer.            |
| `writer_mut()`       | `&mut W`      | Borrow the writer mutably.               |
| `into_inner()`       | `W`           | Consume the encoder, return the writer.  |

**Example:**

```rust
use pack_io::IoEncoder;

let mut sink: Vec<u8> = Vec::new();
let mut enc = IoEncoder::new(&mut sink);
enc.write(&42_u64).unwrap();
enc.write(&"hello").unwrap();
assert!(!sink.is_empty());
```

### `IoDecoder`

```rust,ignore
pub struct IoDecoder<R: Read> { /* … */ }
```

| Constructor                              | Purpose                                              |
|------------------------------------------|------------------------------------------------------|
| `IoDecoder::new(reader)`                 | Default `Config` (`max_alloc = 1 GiB`).              |
| `IoDecoder::with_config(reader, cfg)`    | Validates `cfg`; returns `Err` if `max_alloc == 0`.  |

| Method               | Returns       | Purpose                                  |
|----------------------|---------------|------------------------------------------|
| `read::<T>()`        | `Result<T>`   | Decode one value of `T` from the reader. |
| `reader()`           | `&R`          | Borrow the underlying reader.            |
| `into_inner()`       | `R`           | Consume the decoder, return the reader.  |

**Example — round-trip through a `Cursor<Vec<u8>>`:**

```rust
use pack_io::{IoEncoder, IoDecoder};
use std::io::Cursor;

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

### `encode_into`

```rust,ignore
pub fn encode_into<T: Serialize + ?Sized, W: Write>(
    value: &T, writer: &mut W,
) -> Result<()>;
```

Single-shot convenience wrapper around [`IoEncoder::write`](#ioencoder).

**Example:**

```rust
use pack_io::encode_into;

let mut buf: Vec<u8> = Vec::new();
encode_into(&(7_u64, "hello"), &mut buf).unwrap();
assert!(!buf.is_empty());
```

### `decode_from`

```rust,ignore
pub fn decode_from<T: Deserialize, R: Read>(reader: &mut R) -> Result<T>;
```

Read all remaining bytes from `reader` and decode them as a single value
of type `T`. Returns [`SerialError::TrailingBytes`](#serialerror) if the
reader yielded extra bytes after the value was decoded.

**Example:**

```rust
use pack_io::{encode, decode_from};
use std::io::Cursor;

let bytes = encode(&42_u64).unwrap();
let n: u64 = decode_from(&mut Cursor::new(bytes)).unwrap();
assert_eq!(n, 42);
```

---

## Tier 3 — the trait surface

### `Encode` / `Decode` (behaviour traits)

```rust,ignore
pub trait Encode {
    fn write_byte(&mut self, byte: u8) -> Result<()>;
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()>;
    fn reserve(&mut self, _additional: usize) { /* default: no-op */ }
    fn write_varint_u64(&mut self, value: u64) -> Result<()> { /* default */ }
    fn write_varint_u128(&mut self, value: u128) -> Result<()> { /* default */ }
}

pub trait Decode {
    fn read_byte(&mut self) -> Result<u8>;
    fn read_into(&mut self, out: &mut [u8]) -> Result<()>;
    fn max_alloc(&self) -> usize;
    fn read_varint_u64(&mut self) -> Result<u64> { /* default */ }
    fn read_varint_u128(&mut self) -> Result<u128> { /* default */ }
    fn read_length_prefixed(&mut self) -> Result<Vec<u8>> { /* default */ }
}
```

The behavioural seam — implemented by every concrete encoder / decoder in
the crate. User code rarely implements these directly; they exist so that
[`Serialize`](#serialize--deserialize-value-traits) and
[`Deserialize`](#serialize--deserialize-value-traits) impls can be written
generically and work through both the in-memory and streaming codec
flavours unchanged.

### `Serialize` / `Deserialize` (value traits)

```rust,ignore
pub trait Serialize {
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()>;
}

pub trait Deserialize: Sized {
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self>;
}
```

**Contract:**

- `serialize` MUST be deterministic: equal values produce equal bytes.
- `serialize` appends to the encoder's buffer / sink; it does not clear
  what's already there.
- `deserialize` consumes exactly the bytes a corresponding `serialize`
  would have produced.
- On malformed input, `deserialize` MUST return an error and MUST NOT
  panic, allocate unboundedly, or read past the underlying source.

**Example — implement both traits for a custom struct:**

```rust
use pack_io::{Decode, Decoder, Deserialize, Encode, Encoder, Result, Serialize};

#[derive(Debug, PartialEq)]
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

let p = Point { x: 3, y: -7 };
let bytes = pack_io::encode(&p).unwrap();
let back: Point = pack_io::decode(&bytes).unwrap();
assert_eq!(back, p);
```

**Example — the same `Point` impl works through `IoEncoder<File>`:**

```rust,no_run
# use pack_io::{Decode, Encode, Result, Serialize, Deserialize};
# #[derive(PartialEq, Debug)] struct Point { x: i32, y: i32 }
# impl Serialize for Point {
#     fn serialize<E: Encode + ?Sized>(&self, e: &mut E) -> Result<()> {
#         self.x.serialize(e)?;
#         self.y.serialize(e)
#     }
# }
# impl Deserialize for Point {
#     fn deserialize<D: Decode + ?Sized>(d: &mut D) -> Result<Self> {
#         Ok(Point { x: i32::deserialize(d)?, y: i32::deserialize(d)? })
#     }
# }
use pack_io::IoEncoder;
use std::fs::File;

let file = File::create("point.pack").unwrap();
let mut enc = IoEncoder::new(file);
enc.write(&Point { x: 3, y: -7 }).unwrap();
```

### `#[derive(Serialize, Deserialize)]`

```rust,ignore
use pack_io::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Account { id: u64, handle: String, active: bool }

#[derive(Serialize, Deserialize)]
enum Event {
    Heartbeat,
    Login { user: u64, ip: String },
    Error(u32, String),
}
```

The derive macros (feature `derive`, default off) write sound `Serialize`
and `Deserialize` impls for any struct (named, tuple, unit) and any enum
(any variant shape), generic over type parameters.

Field order in the source code is the encoded byte order. Enums are
encoded as `varint(variant_index) ++ fields`, where `variant_index` is
the variant's source-declaration position starting at `0`. **Append new
variants to the end** of an enum declaration to preserve wire-format
compatibility — inserting a variant in the middle shifts the indices of
every later variant and breaks the encoding for that enum.

Unknown variant indices on decode surface as
[`SerialError::UnknownVariant`](#serialerror).

The macros are re-exported at `pack_io::{Serialize, Deserialize,
DeserializeView}` — the underlying [`pack-io-derive`](https://crates.io/crates/pack-io-derive)
proc-macro crate is an implementation detail.

---

## Zero-copy decode

The owning [`Deserialize`](#serialize--deserialize-value-traits) surface
allocates `String`s and `Vec<u8>`s during decode. The zero-copy
[`DeserializeView`](#deserializeview) surface returns `&'a str` / `&'a [u8]`
that borrow directly from the input slice — no per-field allocation. Both
surfaces use the **same on-wire format**; choose the surface that matches
the lifetime relationship the caller has with the source bytes.

On a representative borrow-heavy record, local Criterion microbenchmarks
show `decode_view` running **~7×** faster than the owning `decode` path,
and **~14×** faster for a 64-byte `String` round-trip. Reproduce with
`cargo bench --bench codec_bench --features derive`.

### `DeserializeView`

```rust,ignore
pub trait DeserializeView<'a>: Sized {
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self>;
}
```

The borrowed counterpart to [`Deserialize`](#serialize--deserialize-value-traits).
`'a` is the lifetime of the input buffer; the borrow checker guarantees
the decoded value cannot outlive its source.

Built-in implementors:

- `&'a str`, `&'a [u8]` — the headline zero-copy types.
- Every primitive (`u8` … `u128`, `i8` … `i128`, `usize`, `isize`,
  `bool`, `f32`, `f64`, `()`, `String`) — `DeserializeView` reduces to
  `Deserialize` for these (no borrow involved).
- `Option<T>`, `Result<T, E>`, tuples (arity 1–12), fixed arrays `[T; N]`.
- `Vec<T>`, `BTreeMap<K, V>`, `BTreeSet<T>`, `HashMap<K, V>` *(std)*,
  `HashSet<T>` *(std)* — container allocated, elements may still borrow.

### `decode_view`

```rust,ignore
pub fn decode_view<'a, T: DeserializeView<'a>>(bytes: &'a [u8]) -> Result<T>;
```

Tier-1 zero-copy entry point — the symmetric counterpart to
[`decode`](#decode). Strict: rejects trailing bytes with
[`SerialError::TrailingBytes`](#serialerror).

**Example:**

```rust
let bytes = pack_io::encode(&"hello").unwrap();
let view: &str = pack_io::decode_view(&bytes).unwrap();
assert_eq!(view, "hello"); // borrowed from `bytes`
```

### `Decoder::read_length_prefixed_borrowed`

```rust,ignore
impl<'a> Decoder<'a> {
    pub fn read_length_prefixed_borrowed(&mut self) -> Result<&'a [u8]>;
}
```

Inherent method on the in-memory [`Decoder<'a>`](#decoder) — reads a
varint length prefix, validates it against [`Config::max_alloc`](#config)
and the remaining input, then returns a borrowed slice over the next
`length` bytes. Powers the `&'a str` / `&'a [u8]` implementations of
[`DeserializeView`](#deserializeview).

Not available on [`IoDecoder<R>`](#iodecoder) — streaming sources have
no buffer to borrow from. Reach for [`Decoder`](#decoder) when zero-copy
matters.

### `#[derive(DeserializeView)]`

```rust,ignore
use pack_io::{DeserializeView, decode_view};

#[derive(DeserializeView)]
struct ViewMsg<'a> {
    id: u64,
    text: &'a str,
    payload: &'a [u8],
}
```

The derive (feature `derive`, default off) writes a sound
[`DeserializeView`](#deserializeview) impl for any struct that has
exactly one lifetime parameter. Each field type must already implement
`DeserializeView<'that_lifetime>` — the built-in impls cover primitives,
`&'a str`, `&'a [u8]`, and the standard container types.

Enum support is planned for a later minor release.

---

## Schema evolution

A type tagged with `#[pack_io(version = N)]` opts into a length-framed
encoding (`varint(version) ++ varint(body_len) ++ body`) that lets old
and new revisions of the type interoperate as long as field changes are
**append-only**. Old decoders skip trailing body bytes they don't
recognise; new decoders default-construct fields that older payloads
didn't include.

Pulls in the `schema` Cargo feature (which transitively enables
`derive`).

Full normative spec: [`docs/WIRE_FORMAT.md §3.8`](./WIRE_FORMAT.md#38-versioned-structs).

### `#[pack_io(version)]`

Type-level attribute marking the struct as versioned. `N` is a positive
`u32`; `0` is rejected.

```rust,ignore
#[derive(pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 2)]
struct Message {
    id: u64,
    text: String,
    #[pack_io(since = 2)]
    timestamp: Option<u64>,
}
```

When present, the encoded payload is `varint(N) ++ varint(body_len) ++
body`. When absent, the type uses the plain v0.4 field-concatenation
encoding. The choice is per-type and cannot be changed later without
breaking the wire format for that type.

### `#[pack_io(since)]`

Field-level attribute marking the field as **added** at version `N`.
Defaults to `1` (always present). Requires the field's type to
implement `Default`, since decoders reading payloads from version
`< N` use `Default::default()` for the missing field.

### `#[pack_io(deprecated)]`

Field-level attribute marking the field as **removed** at version `N`.
The field MUST remain in the struct declaration; encoders at version
`>= N` simply drop the field, and decoders at version `>= N`
default-construct it.

Pre-conditions:

- `deprecated > since` (a field cannot be removed before it is
  introduced). The derive macro errors at compile time if violated.
- The field's type must implement `Default`.

### `peek_version`

```rust,ignore
pub fn peek_version(bytes: &[u8]) -> Result<u32>;
```

Read only the leading `varint(version)` of a versioned payload without
consuming the buffer or decoding the body. Useful when a single
transport carries multiple revisions and the dispatcher needs to pick a
target type at runtime.

```rust
# #[cfg(feature = "schema")] {
use pack_io::{encode, peek_version, Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
#[pack_io(version = 2)]
struct Msg { id: u64 }

let bytes = encode(&Msg { id: 7 }).unwrap();
assert_eq!(peek_version(&bytes).unwrap(), 2);
# }
```

On a non-versioned payload, `peek_version` returns whatever the first
varint of the payload happens to be — call it only on payloads you
know are versioned.

---

## Errors

### `SerialError`

```rust,ignore
#[non_exhaustive]
pub enum SerialError {
    UnexpectedEof   { needed: usize, remaining: usize },
    InvalidLength   { declared: u64, remaining: usize },
    VarintOverflow,
    IntegerOutOfRange,
    InvalidBool     { byte: u8 },
    InvalidUtf8,
    InvalidTag      { kind: &'static str, tag: u8 },
    UnknownVariant  { kind: &'static str, index: u64 },
    TrailingBytes   { remaining: usize },
    #[cfg(feature = "std")]
    Io              { kind: std::io::ErrorKind, message: String },
}
```

`#[non_exhaustive]` so future MINOR releases can add variants without
breaking downstream `match` arms.

| Variant | When | Caller action |
|---------|------|---------------|
| `UnexpectedEof` | Decoder needed more bytes than were available. | Treat as truncated transport. |
| `InvalidLength` | A length prefix exceeded the buffer or `max_alloc`. | Reject the message. |
| `VarintOverflow` | A LEB128 varint exceeded its target width. | Reject. |
| `IntegerOutOfRange` | A decoded integer did not fit in the requested narrower target. | Reject; producer / consumer width mismatch. |
| `InvalidBool` | A boolean byte was neither `0x00` nor `0x01`. | Reject. |
| `InvalidUtf8` | A length-prefixed byte run was not valid UTF-8. | Reject. |
| `InvalidTag` | An `Option` / `Result` tag was outside `0x00` / `0x01`. | Reject. |
| `UnknownVariant` | An enum variant index was outside the declared variants of the target type (v0.4+). | Reject; producer / consumer are at incompatible enum revisions. |
| `TrailingBytes` | Strict [`decode`](#decode) left bytes unread. | Either producer wrong, or use [`Decoder`](#decoder) for multi-value streams. |
| `Io` *(std-only)* | Underlying `Read` / `Write` failed. | Inspect `kind` and `message`; surface to transport. |

`SerialError` implements `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`,
and (under the default `std` feature) `std::error::Error`. Error messages
never echo the offending bytes back to the caller — safe to log without
input sanitisation.

### `Result<T>`

```rust,ignore
pub type Result<T> = core::result::Result<T, SerialError>;
```

Convenience alias used throughout the codec.

---

## Supported types

| Group | Types |
|-------|-------|
| Unsigned integers | `u8`, `u16`, `u32`, `u64`, `u128`, `usize` |
| Signed integers   | `i8`, `i16`, `i32`, `i64`, `i128`, `isize` |
| Floats            | `f32`, `f64` |
| Bool / unit       | `bool`, `()` |
| Strings           | `String`, `&str` (encode only) |
| Sequences         | `Vec<T>`, `&[T]` (encode only), `[T; N]` |
| Tuples            | arity 1 through 12 |
| Sums              | `Option<T>`, `Result<T, E>` |
| Maps              | `BTreeMap<K, V>`, `HashMap<K, V, S>` *(std)* |
| Sets              | `BTreeSet<T>`, `HashSet<T, S>` *(std)* |
| References        | `&T` where `T: Serialize` (encode only) |

User-defined types opt in via `#[derive(pack_io::Serialize,
pack_io::Deserialize)]` under the `derive` feature.

---

## Wire format

The full normative byte-level spec lives in
[`docs/WIRE_FORMAT.md`](./WIRE_FORMAT.md). Highlights:

- LEB128 varint for multi-byte unsigned integers, ZigZag-then-LEB128 for
  signed. Same shape as `protobuf`, `postcard`, and `bincode`'s varint mode.
- 1 byte fixed for `u8` / `i8` (no varint overhead for standalone bytes).
- IEEE 754 bit pattern, little-endian for `f32` / `f64`.
- `String` / sequences / collections: varint length prefix + content.
- `Option` / `Result` / `bool`: strict 1-byte tag (`0x00` / `0x01`).
- **Hash-based collections encode in canonical key-sorted order** —
  sorted lexicographically by the encoded key bytes. A `HashMap` and a
  `BTreeMap` over the same logical data encode identically.

The format is frozen for the `1.x` line as of `v0.3.0`. Wire-format
changes in any `1.x` release are prohibited; any change after `1.0` ships
that breaks the format requires a `2.x` major version bump.

---

## Feature flags

| Feature  | Default | Description |
|----------|---------|-------------|
| `std`    | yes     | Standard library. Off → `no_std`. Enables [`std::error::Error`] on [`SerialError`](#serialerror), `HashMap` / `HashSet` integration, and the [`io`](#tier-2b--streaming-codec) module. |
| `derive` | no      | `#[derive(Serialize, Deserialize, DeserializeView)]` proc-macros. Pulls in the companion `pack-io-derive` crate. |
| `schema` | no      | Schema-evolution attributes (`#[pack_io(version = N)]`, `#[pack_io(since = N)]`, `#[pack_io(deprecated = N)]`). Implies `derive`. |
| `serde`  | no      | Optional `serde` interop shims. |

All feature flags are **additive**. Enabling a feature never removes or
changes existing behaviour; disabling a feature never breaks code that did
not opt into it.

---

## Other public items

### `VERSION`

```rust,ignore
pub const VERSION: &str;
```

The semantic version of the crate, exposed at compile time. Mirrors
`Cargo.toml` exactly.

**Example:**

```rust
assert!(pack_io::VERSION.starts_with("0."));
```

---

## Cross-platform notes

- Tier-1 supported targets: Linux (`x86_64`, `aarch64`), macOS (`x86_64`,
  Apple Silicon), Windows (`x86_64`). All three run the full CI matrix on
  every commit, on both stable and MSRV.
- Encoding is byte-deterministic across all three.
- `usize` and `isize` encode through `u64` and `i64` respectively, so a
  value encoded on a 64-bit machine that exceeds `usize::MAX` on a 32-bit
  consumer surfaces as [`SerialError::IntegerOutOfRange`](#serialerror) —
  no silent truncation.
- `no_std` builds rely on `core` + `alloc` only. The Tier-2b streaming
  codec ([`IoEncoder`](#ioencoder) / [`IoDecoder`](#iodecoder),
  [`encode_into`](#encode_into) / [`decode_from`](#decode_from)) requires
  `std` and is gated on the `std` feature.
- `HashMap` / `HashSet` impls require `std` and are gated on the same
  feature.

---

## Compatibility & semver

- **API frozen as of v0.7.0.** The complete public surface listed in
  [§ Frozen public surface](#frozen-public-surface) is the v1.0
  contract. Source-breaking changes are deferred to v2.0.
- **Wire format frozen at v0.3.0**, currently spec version 1.2. Any
  change that affects the wire shape is prohibited until the 2.x line.
- Pre-1.0 minor releases (v0.7.x → v0.9.x) ship bug fixes, hardening
  passes, performance work, and strictly *additive* changes (e.g.
  new `SerialError` variants under the existing `#[non_exhaustive]`
  enum, new derive macro support for new field types).
- Post-1.0: SemVer in the strict sense. Breaking changes bump MAJOR;
  the wire format never breaks within a MAJOR.
- Deprecated items remain available for at least one MAJOR after the
  `#[deprecated]` attribute is added.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
