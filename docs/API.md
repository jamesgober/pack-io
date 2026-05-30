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
- [Tier 1 — the lazy path](#tier-1--the-lazy-path)
  - [`encode`](#encode)
  - [`decode`](#decode)
- [Tier 2 — the configured path](#tier-2--the-configured-path)
  - [`Encoder`](#encoder)
  - [`Decoder`](#decoder)
  - [`Config`](#config)
- [Tier 3 — the power path](#tier-3--the-power-path)
  - [`Serialize` / `Deserialize` traits](#serialize--deserialize-traits)
  - [`View<T>` zero-copy decode](#view-zero-copy-decode) _(planned: 0.4)_
- [Errors](#errors)
  - [`SerialError`](#serialerror)
  - [`Result<T>`](#resultt)
- [Supported types](#supported-types)
- [Wire format](#wire-format)
- [Schema evolution](#schema-evolution) _(planned: 0.5)_
- [Feature flags](#feature-flags)
- [Other public items](#other-public-items)
- [Cross-platform notes](#cross-platform-notes)
- [Compatibility & semver](#compatibility--semver)

---

## Installation

```toml
[dependencies]
pack-io = "0.2"
```

`no_std` build:

```toml
[dependencies]
pack-io = { version = "0.2", default-features = false }
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
- **Tier 2** — the [`Encoder`](#encoder) and [`Decoder`](#decoder) structs.
  Re-use a single buffer across many encodes; read several values from one
  buffer; validate configuration ([`Config`](#config)) at construction time.
- **Tier 3** — the [`Serialize`](#serialize--deserialize-traits) and
  [`Deserialize`](#serialize--deserialize-traits) traits, implemented today
  by hand on user types and automatically by the derive macro arriving in
  `0.4`.

Properties that hold across every release:

- The encode hot path never allocates beyond the output buffer.
- Encoding is **deterministic** — the same value always produces the same
  bytes, regardless of platform, insertion order, or build flags.
- Decoding is **safe under untrusted input** — bounded allocation, validated
  length prefixes, no panics, no reads past the input. Enforced by the
  `proptest` adversarial-decode harness in [`tests/adversarial.rs`](../tests/adversarial.rs).

---

## Tier 1 — the lazy path

### `encode`

```rust,ignore
pub fn encode<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>>;
```

Encode `value` into a freshly allocated `Vec<u8>`. The buffer is sized
exactly to fit the encoded value — no spare capacity.

**Parameters:**

| Name    | Type      | Description                                            |
|---------|-----------|--------------------------------------------------------|
| `value` | `&T`      | The value to encode. Borrowed, never cloned.           |

**Returns:** `Ok(Vec<u8>)` on success; `Err(SerialError)` when the type's
`Serialize` impl reports a failure. The built-in primitive impls are
infallible, so for those this never errors.

**Example — round-trip a primitive:**

```rust
let bytes = pack_io::encode(&42_u64).unwrap();
let back: u64 = pack_io::decode(&bytes).unwrap();
assert_eq!(back, 42);
```

**Example — round-trip a heterogeneous tuple:**

```rust
let v = (1_u64, true, String::from("hello"));
let bytes = pack_io::encode(&v).unwrap();
let back: (u64, bool, String) = pack_io::decode(&bytes).unwrap();
assert_eq!(back, v);
```

**Example — encode an `Option`:**

```rust
let bytes_some = pack_io::encode(&Some(42_u64)).unwrap();
let bytes_none = pack_io::encode::<Option<u64>>(&None).unwrap();
assert_ne!(bytes_some, bytes_none);
```

### `decode`

```rust,ignore
pub fn decode<T: Deserialize>(bytes: &[u8]) -> Result<T>;
```

Decode a value of type `T` from `bytes`, requiring the input to be fully
consumed. After the value is read, the decoder verifies that no bytes
remain; trailing input is reported as
[`SerialError::TrailingBytes`](#serialerror).

Callers that want to read several values from a single buffer should use
[`Decoder`](#decoder) directly.

**Parameters:**

| Name    | Type    | Description                                          |
|---------|---------|------------------------------------------------------|
| `bytes` | `&[u8]` | The encoded input. Read-only, validated as consumed. |

**Returns:** `Ok(T)` on success; `Err(SerialError)` for any malformed input
(short read, oversized length prefix, invalid variant tag, trailing bytes,
…). Never panics, never allocates unboundedly, never reads past `bytes`.

**Example — strict decode rejects trailing bytes:**

```rust
let mut bytes = pack_io::encode(&7_u8).unwrap();
bytes.push(0xff); // stray byte
let err = pack_io::decode::<u8>(&bytes).unwrap_err();
assert!(matches!(err, pack_io::SerialError::TrailingBytes { remaining: 1 }));
```

**Example — safely handle untrusted input:**

```rust
let untrusted: &[u8] = &[0xff, 0xff, 0x0f]; // hostile-looking varint
match pack_io::decode::<u64>(untrusted) {
    Ok(n)    => println!("decoded {n}"),
    Err(err) => println!("rejected: {err}"),
}
```

---

## Tier 2 — the configured path

### `Encoder`

```rust,ignore
pub struct Encoder { /* … */ }
```

Buffered encoder. Writes into an owned `Vec<u8>`; the buffer can be swapped
in / out so a single allocation serves many encodes.

**Constructors:**

| Name                          | Purpose                                                |
|-------------------------------|--------------------------------------------------------|
| `Encoder::new()`              | Empty buffer, default settings.                        |
| `Encoder::into_buffer(buf)`   | Append into the caller's `Vec<u8>` — avoids alloc.     |
| `Encoder::default()`          | Same as `new()`.                                       |

**Methods:**

| Name                  | Returns                       | Purpose                                |
|-----------------------|-------------------------------|----------------------------------------|
| `write(&value)`       | `Result<()>`                  | Append `value`'s wire-format bytes.    |
| `as_bytes()`          | `&[u8]`                       | Borrow the bytes written so far.       |
| `into_inner()`        | `Vec<u8>`                     | Consume the encoder, return the buf.   |
| `take()`              | `Vec<u8>`                     | Swap in an empty buffer, return old.   |

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

**Example — re-use a caller-owned buffer across many encodes:**

```rust
use pack_io::Encoder;

let mut buf = Vec::with_capacity(256);
for round in 0..3 {
    let mut enc = Encoder::into_buffer(buf);
    enc.write(&(round as u64)).unwrap();
    enc.write(&format!("round-{round}")).unwrap();
    buf = enc.into_inner();
    // … send `buf` somewhere …
    buf.clear();
}
```

### `Decoder`

```rust,ignore
pub struct Decoder<'a> { /* borrows from &'a [u8] */ }
```

Cursored decoder. Borrows from an input slice and advances a position
pointer as values are read. Bounds-checked on every operation.

**Constructors:**

| Name                                | Purpose                                              |
|-------------------------------------|------------------------------------------------------|
| `Decoder::new(bytes)`               | Default config (`max_alloc = 1 GiB`).                |
| `Decoder::with_config(bytes, cfg)`  | Validates `cfg`; returns `Err` if `max_alloc == 0`.  |

**Methods:**

| Name           | Returns          | Purpose                                  |
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
assert_eq!((n, b), (7, true));
assert!(dec.is_empty());
```

**Example — reject hostile length prefixes before allocation:**

```rust
use pack_io::{Decoder, Config, SerialError};

let cfg = Config::new().with_max_alloc(4 * 1024);
let bytes = &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]; // length = u64::MAX
let mut dec = Decoder::with_config(bytes, cfg).unwrap();
let err = dec.read::<String>().unwrap_err();
assert!(matches!(
    err,
    SerialError::InvalidLength { .. } | SerialError::UnexpectedEof { .. }
));
```

### `Config`

```rust,ignore
#[non_exhaustive]
pub struct Config {
    pub max_alloc: usize,
}
```

Configuration for a decode session. `#[non_exhaustive]` so future MINOR
releases can add knobs without breaking downstream code.

**Constructors:**

| Name                          | Purpose                                                |
|-------------------------------|--------------------------------------------------------|
| `Config::new()`               | Default — `max_alloc = 1 GiB`. `const fn`.             |
| `Config::default()`           | Same as `new()`.                                       |
| `Config::with_max_alloc(n)`   | Replace `max_alloc`, return updated config. `const fn`.|

**Field:**

| Name        | Type    | Description                                              |
|-------------|---------|----------------------------------------------------------|
| `max_alloc` | `usize` | Largest single length-prefixed value the decoder accepts. Hostile producers that send multi-gigabyte length prefixes fail fast. |

The default of 1 GiB is large enough to be irrelevant for well-formed
inputs and small enough to refuse the obvious `length = u64::MAX` attack
before allocating a single byte. Tighten in any context that accepts
untrusted input from a low-budget producer.

**Example — build a tight config:**

```rust
let cfg = pack_io::Config::new().with_max_alloc(16 * 1024);
assert_eq!(cfg.max_alloc, 16 * 1024);
```

**Example — zero cap is rejected at construction:**

```rust
let cfg = pack_io::Config::new().with_max_alloc(0);
assert!(pack_io::Decoder::with_config(&[], cfg).is_err());
```

---

## Tier 3 — the power path

### `Serialize` / `Deserialize` traits

```rust,ignore
pub trait Serialize {
    fn serialize(&self, encoder: &mut Encoder) -> Result<()>;
}

pub trait Deserialize: Sized {
    fn deserialize(decoder: &mut Decoder<'_>) -> Result<Self>;
}
```

The seam between a Rust value and its wire-format bytes. Built-in primitive
impls live in `src/impls.rs`. User-defined types implement these traits
directly today; the `derive` macro (feature `derive`, lands in `0.4`)
writes a sound implementation automatically.

**Contracts:**

- `serialize` MUST be deterministic: equal values produce equal bytes.
- `serialize` appends to the encoder's buffer — it does not clear it first.
- `deserialize` consumes exactly the bytes that a corresponding `serialize`
  would have produced.
- On malformed input, `deserialize` MUST return an error and MUST NOT panic,
  allocate unboundedly, or read past the underlying buffer.

**Example — implement both traits for a custom struct:**

```rust
use pack_io::{Encoder, Decoder, Serialize, Deserialize, SerialError};

#[derive(Debug, PartialEq)]
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

let p = Point { x: 3, y: -7 };
let bytes = pack_io::encode(&p).unwrap();
let back: Point = pack_io::decode(&bytes).unwrap();
assert_eq!(back, p);
```

**Example — implement `Serialize` for a borrowed view (no `Deserialize` yet):**

```rust
use pack_io::{Encoder, Serialize, SerialError};

struct LabeledBytes<'a> { label: &'a str, payload: &'a [u8] }

impl Serialize for LabeledBytes<'_> {
    fn serialize(&self, enc: &mut Encoder) -> Result<(), SerialError> {
        self.label.serialize(enc)?;
        self.payload.serialize(enc)
    }
}

let lb = LabeledBytes { label: "hdr", payload: b"\x01\x02\x03" };
let bytes = pack_io::encode(&lb).unwrap();
assert!(!bytes.is_empty());
```

### `View<T>` zero-copy decode

_Planned: v0.4.0._ Zero-copy view types that borrow string and byte fields
directly from the input buffer.

```rust,ignore
pub struct View<'a, T> { /* borrows from &'a [u8] */ }
pub fn decode_view<'a, T>(bytes: &'a [u8]) -> Result<View<'a, T>>;
```

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
}
```

`#[non_exhaustive]` so future MINOR releases can add variants without
breaking downstream `match` arms. Callers MUST include a wildcard arm.

| Variant | When | What the caller does |
|---------|------|----------------------|
| `UnexpectedEof` | Decoder needed more bytes than were available. | Treat as a truncated transport; drop / re-read. |
| `InvalidLength` | A length prefix exceeded the buffer or the configured `max_alloc`. | Reject the message; the producer is hostile or buggy. |
| `VarintOverflow` | A LEB128 varint exceeded its target width (≥ 10 bytes for `u64`, ≥ 19 for `u128`). | Reject the message. |
| `IntegerOutOfRange` | A decoded `u64` did not fit in the requested narrower target (`u16`, `u32`, `usize` on 32-bit). | Reject; the producer is using a wider type than the consumer expects. |
| `InvalidBool` | A boolean byte was neither `0x00` nor `0x01`. | Reject the message. |
| `InvalidUtf8` | A length-prefixed byte run was not valid UTF-8 when decoding a `String`. | Reject the message. |
| `InvalidTag` | An `Option` / `Result` tag byte was outside the legal `0x00` / `0x01` range. | Reject the message. |
| `TrailingBytes` | A strict [`decode`](#decode) call left bytes unread. | Either the producer is wrong, or the caller should be using [`Decoder`](#decoder) directly to read multiple values. |

`SerialError` implements `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`, and
(with `feature = "std"`, default-on) `std::error::Error`. Error messages
never echo the offending bytes back to the caller — safe to log without
input sanitisation.

**Example — match by variant:**

```rust
use pack_io::{decode, SerialError};

match decode::<bool>(&[0x7f]) {
    Ok(_) => unreachable!(),
    Err(SerialError::InvalidBool { byte }) => assert_eq!(byte, 0x7f),
    Err(other) => panic!("unexpected variant: {other}"),
}
```

### `Result<T>`

```rust,ignore
pub type Result<T> = core::result::Result<T, SerialError>;
```

Convenience alias used throughout the codec.

---

## Supported types

Primitive `Serialize` / `Deserialize` impls included in `v0.2.0`:

| Group | Types |
|-------|-------|
| Unsigned integers | `u8`, `u16`, `u32`, `u64`, `u128`, `usize` |
| Signed integers   | `i8`, `i16`, `i32`, `i64`, `i128`, `isize` |
| Floats            | `f32`, `f64` |
| Bool / unit       | `bool`, `()` |
| Strings           | `String`, `&str` (encode only) |
| Bytes             | `Vec<u8>`, `&[u8]` (encode only) |
| Arrays            | `[T; N]` where `T: Serialize` / `Deserialize` |
| Tuples            | arity 1 through 12 |
| Sums              | `Option<T>`, `Result<T, E>` |
| References        | `&T` where `T: Serialize` (encode only) |

Collections (`Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`, `HashSet<T>`,
`BTreeSet<T>`) arrive at `0.3` along with the wire-format freeze and
streaming `Read` / `Write` integration.

---

## Wire format

| Group | Shape |
|-------|-------|
| `u8`, `i8` | 1 byte (i8 is two's-complement). |
| `u16` … `u64`, `usize` | LEB128 varint (1 – 10 bytes). |
| `u128` | LEB128 varint (1 – 19 bytes). |
| `i16` … `i64`, `isize` | ZigZag map → LEB128 varint. |
| `i128` | ZigZag map → LEB128 varint (1 – 19 bytes). |
| `bool` | 1 byte, `0x00` or `0x01`. Any other byte → `InvalidBool`. |
| `f32` | 4 bytes, IEEE 754 bit pattern, little-endian. |
| `f64` | 8 bytes, IEEE 754 bit pattern, little-endian. |
| `String`, `&str` | varint length prefix, then UTF-8 bytes. |
| `Vec<u8>`, `&[u8]` | varint length prefix, then raw bytes. |
| `[T; N]` | `N` consecutive `T` encodings, no length prefix (`N` is in the type). |
| tuples | fields concatenated in declaration order, no prefix. |
| `Option<T>` | 1 tag byte (`0x00` = `None`, `0x01` = `Some`) then the inner value when present. |
| `Result<T, E>` | 1 tag byte (`0x00` = `Ok`, `0x01` = `Err`) then the inner value. |
| `()` | 0 bytes. |

Floats preserve their IEEE 754 bit pattern exactly — NaN, ±Inf, subnormals,
and signed zeros all round-trip bit-for-bit. Consumers that care about
IEEE equality should compare via `to_bits()`, since `NaN != NaN` by spec.

The full normative spec lands as [`docs/WIRE_FORMAT.md`](./WIRE_FORMAT.md)
at `v0.3.0` when the format freezes. Until then the encoding is considered
**unstable** across the `0.x` series; wire-format-breaking changes are
called out prominently in [`CHANGELOG.md`](../CHANGELOG.md).

---

## Schema evolution

_Planned: v0.5.0._ Producers and consumers at different revisions of a type
remain interoperable as long as the changes are additive.

| Attribute | Where | Meaning |
|-----------|-------|---------|
| `#[pack_io(version = N)]` | struct / enum | The current schema revision. Encoded in the payload header. |
| `#[pack_io(since = N)]` | field | Field was added in version `N`. Older readers skip it; older writers emit no bytes for it. |
| `#[pack_io(deprecated = N)]` | field | Field was removed in version `N`. Newer readers fill defaults. |

---

## Feature flags

| Feature  | Default | Description |
|----------|---------|-------------|
| `std`    | yes     | Standard library. Off → `no_std`. Enables [`std::error::Error`] on [`SerialError`](#serialerror). |
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
`Cargo.toml` exactly via the `CARGO_PKG_VERSION` environment variable —
no parsing, no allocation.

**Example:**

```rust
assert!(pack_io::VERSION.starts_with("0."));
```

---

## Cross-platform notes

- Tier-1 supported targets: Linux (`x86_64`, `aarch64`), macOS (`x86_64`,
  Apple Silicon), Windows (`x86_64`). All three run the full CI matrix on
  every commit, on both stable and MSRV.
- Encoding is byte-deterministic across all three. There is no
  `#[cfg(target_os = …)]` branch on the encode or decode path.
- `usize` and `isize` encode through `u64` and `i64` respectively, so a
  value encoded on a 64-bit machine that exceeds `usize::MAX` on a 32-bit
  consumer surfaces as [`SerialError::IntegerOutOfRange`](#serialerror) —
  no silent truncation.
- `no_std` builds rely on `core` + `alloc` only — no `std::io`, no thread
  locals, no `Instant`. The streaming `Read` / `Write` integration in `0.3`
  will be gated on `std`.

---

## Compatibility & semver

- Pre-1.0: breaking changes bump MINOR (per the project versioning
  strategy). They are called out under their own subheading in the
  changelog. Wire-format changes inside the `0.x` series are also flagged
  prominently.
- Post-1.0: SemVer in the strict sense. Breaking changes bump MAJOR; the
  wire format never breaks within a MAJOR.
- Deprecated items remain available for at least one MAJOR after the
  `#[deprecated]` attribute is added.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
