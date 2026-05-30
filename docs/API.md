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
> **Status: pre-1.0, wire format frozen at v0.3.0.** This document tracks the
> API surface as it lands across the `0.x` series. Sections marked
> _(planned: vX.Y)_ describe the intended surface and are filled in as each
> roadmap phase ships. Items not marked _planned_ are present in the version
> listed in [`Cargo.toml`](../Cargo.toml) and exercised by the test suite.

---

## Table of Contents

- [Installation](#installation)
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
  - [`View<T>` zero-copy decode](#view-zero-copy-decode) _(planned: 0.4)_
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
pack-io = "0.3"
```

`no_std` build:

```toml
[dependencies]
pack-io = { version = "0.3", default-features = false }
```

MSRV is **Rust 1.85** (2024 edition). The CI matrix runs every supported
platform on both stable and MSRV; downstream crates may rely on the declared
`rust-version` in [`Cargo.toml`](../Cargo.toml) for resolver-aware version
selection.

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

### `View<T>` zero-copy decode

_Planned: v0.4.0._ Zero-copy view types that borrow string and byte fields
directly from the input buffer.

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

The derive macro lands in `0.4` so user types can opt into the codec
without writing the impls by hand.

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
| `derive` | no      | `#[derive(Serialize, Deserialize)]` proc-macros. _(populated at 0.4)_ |
| `schema` | no      | Schema-versioning and evolution helpers. _(populated at 0.5)_ |
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

- Pre-1.0: breaking changes bump MINOR (per the project versioning
  strategy). They are called out under their own subheading in the
  changelog. The wire format is **frozen** as of `v0.3.0`; any change
  that affects the wire shape is a wire-format-breaking change and is
  prohibited until the `2.x` line.
- Post-1.0: SemVer in the strict sense. Breaking changes bump MAJOR; the
  wire format never breaks within a MAJOR.
- Deprecated items remain available for at least one MAJOR after the
  `#[deprecated]` attribute is added.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
