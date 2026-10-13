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
> **Status: stable (v1.1.0).** The public surface enumerated below is the
> `1.x` contract: items are only ever added in `1.x`, never removed or
> changed. The wire format ([`docs/WIRE_FORMAT.md`](./WIRE_FORMAT.md),
> spec version 1.3) is frozen — every `1.x` decoder reads every encoding
> a `1.x` encoder produced. The performance baseline
> ([`docs/PERFORMANCE_BASELINE.md`](./PERFORMANCE_BASELINE.md)) is the
> contractual reference; a change exceeding 5 % regression on any row
> blocks the merge. Source-breaking and wire-format-breaking changes are
> deferred to `2.x`.

---

## Table of Contents

- [Installation](#installation)
- [Frozen public surface](#frozen-public-surface)
- [Overview](#overview)
- [Decoding untrusted input](#decoding-untrusted-input)
- [Tier 1 — the lazy path](#tier-1--the-lazy-path)
  - [`encode`](#encode)
  - [`decode` / `decode_with_config`](#decode--decode_with_config)
- [Tier 2a — in-memory codec](#tier-2a--in-memory-codec)
  - [`Encoder`](#encoder)
  - [`Decoder`](#decoder)
  - [`Config`](#config)
- [Tier 2b — streaming codec](#tier-2b--streaming-codec)
  - [`IoEncoder<W>`](#ioencoder)
  - [`IoDecoder<R>`](#iodecoder)
  - [`encode_into`](#encode_into)
  - [`decode_from` / `decode_from_with_config`](#decode_from--decode_from_with_config)
- [Tier 3 — the trait surface](#tier-3--the-trait-surface)
  - [`Encode` / `Decode` (behaviour traits)](#encode--decode-behaviour-traits)
  - [`Serialize` / `Deserialize` (value traits)](#serialize--deserialize-value-traits)
  - [`#[derive(Serialize, Deserialize)]`](#derive-serialize-deserialize)
  - [Derive attributes](#derive-attributes)
- [Zero-copy decode](#zero-copy-decode)
  - [`DeserializeView<'a>`](#deserializeview)
  - [`decode_view` / `decode_view_with_config`](#decode_view--decode_view_with_config)
  - [`Decoder::read_length_prefixed_borrowed` / `read_borrowed`](#decoder-borrowed-reads)
  - [`#[derive(DeserializeView)]`](#derive-deserializeview)
- [Schema evolution](#schema-evolution)
  - [`#[pack_io(version = N)]`](#pack_io-version)
  - [`#[pack_io(since = N)]`](#pack_io-since)
  - [`#[pack_io(deprecated = N)]`](#pack_io-deprecated)
  - [`peek_version`](#peek_version)
- [Canonical decoding](#canonical-decoding)
- [serde adapter (feature `serde`)](#serde-adapter-feature-serde)
- [bytes integration (feature `bytes`)](#bytes-integration-feature-bytes)
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
pack-io = "1"

# With the derive macros (including the schema attributes):
pack-io = { version = "1", features = ["derive"] }
```

`no_std` build (derives, including versioned ones, work here too):

```toml
[dependencies]
pack-io = { version = "1", default-features = false, features = ["derive"] }
```

MSRV is **Rust 1.85** (2024 edition). The CI matrix runs every supported
platform on both stable and MSRV; downstream crates may rely on the declared
`rust-version` in [`Cargo.toml`](../Cargo.toml) for resolver-aware version
selection.

---

## Frozen public surface

The exhaustive list below is the `1.x` contract. Anything not on this
list is an internal detail and may change at any time without a major
version bump. Items are only added within `1.x`.

### Types

| Path                              | Kind                | Since |
|-----------------------------------|---------------------|-------|
| `pack_io::Encoder`                | concrete struct     | v0.2.0 |
| `pack_io::Decoder<'a>`            | concrete struct     | v0.2.0 |
| `pack_io::Config`                 | `#[non_exhaustive]` struct | v0.2.0 |
| `pack_io::SerialError`            | `#[non_exhaustive]` enum   | v0.2.0 |
| `pack_io::Result<T>`              | type alias          | v0.2.0 |
| `pack_io::IoEncoder<W: Write>`    | concrete struct     | v0.3.0 |
| `pack_io::IoDecoder<R: Read>`     | concrete struct     | v0.3.0 |
| `pack_io::serde::Compat<T>` *(feature `serde`)* | wrapper struct | v1.1.0 |

### Traits

| Path                              | Methods | Since |
|-----------------------------------|---------|-------|
| `pack_io::Serialize`              | `serialize`, `serialize_slice` (default) | v0.6.0 |
|                                   | `encoded_len`, `encoded_len_slice` (default) | v1.1.0 |
| `pack_io::Deserialize`            | `deserialize`, `deserialize_many` (default) | v0.6.0 |
|                                   | `deserialize_array` (default) | v1.1.0 |
| `pack_io::DeserializeView<'a>`    | `deserialize_view` | v0.4.0 |
|                                   | `deserialize_view_many`, `deserialize_view_array` (default) | v1.1.0 |
| `pack_io::Encode`                 | `write_byte`, `write_bytes`, `reserve`, `write_varint_u64`, `write_varint_u128` | v0.3.0 |
| `pack_io::Decode`                 | `read_byte`, `read_into`, `max_alloc`, `read_varint_u64`, `read_varint_u128`, `read_length_prefixed` | v0.3.0 |
|                                   | `remaining_hint`, `config`, `depth`, `enter_nested`, `leave_nested`, `consumed`, `read_byte_cow` (default) | v1.1.0 |

Every method added in 1.1 has a default implementation, so existing
implementations of these traits keep compiling unchanged.

### Free functions

| Path                              | Since |
|-----------------------------------|-------|
| `pack_io::encode`                 | v0.2.0 |
| `pack_io::decode`                 | v0.2.0 |
| `pack_io::decode_view`            | v0.4.0 |
| `pack_io::encode_into`            | v0.3.0 |
| `pack_io::decode_from`            | v0.3.0 |
| `pack_io::peek_version`           | v0.5.0 |
| `pack_io::decode_with_config`     | v1.1.0 |
| `pack_io::decode_view_with_config`| v1.1.0 |
| `pack_io::decode_from_with_config`| v1.1.0 |
| `pack_io::varint_len` (`const fn`)| v1.1.0 |
| `pack_io::serde::{to_vec, to_encoder, from_slice, from_slice_with_config, from_decoder, from_decode}` *(feature `serde`)* | v1.1.0 |
| `pack_io::serde::compat::{serialize, deserialize, deserialize_view}` *(feature `serde`)* | v1.1.0 |

### Constants

| Path                              | Since |
|-----------------------------------|-------|
| `pack_io::VERSION`                | v0.1.0 |
| `pack_io::Config::DEFAULT_MAX_ALLOC` | v1.1.0 |
| `pack_io::Config::DEFAULT_MAX_DEPTH` | v1.1.0 |

### Inherent methods on `Encoder`

| Method                            | Since |
|-----------------------------------|-------|
| `Encoder::new`                    | v0.2.0 |
| `Encoder::with_capacity`          | v0.6.0 |
| `Encoder::into_buffer`            | v0.2.0 |
| `Encoder::as_bytes`               | v0.2.0 |
| `Encoder::into_inner`             | v0.2.0 |
| `Encoder::take`                   | v0.2.0 |
| `Encoder::write`                  | v0.2.0 |

### Inherent methods on `Decoder<'a>`

| Method                                       | Since |
|----------------------------------------------|-------|
| `Decoder::new`                               | v0.2.0 |
| `Decoder::with_config`                       | v0.2.0 |
| `Decoder::position`                          | v0.2.0 |
| `Decoder::remaining`                         | v0.2.0 |
| `Decoder::is_empty`                          | v0.2.0 |
| `Decoder::read`                              | v0.2.0 |
| `Decoder::read_length_prefixed_borrowed`     | v0.4.0 |
| `Decoder::read_borrowed`                     | v1.1.0 |
| `Decoder::config`                            | v1.1.0 |

### Inherent methods on `IoEncoder<W>` / `IoDecoder<R>`

| Method                                       | Since |
|----------------------------------------------|-------|
| `IoEncoder::new`, `writer`, `writer_mut`, `into_inner`, `write` | v0.3.0 |
| `IoDecoder::new`, `with_config`, `reader`, `into_inner`, `read` | v0.3.0 |
| `IoDecoder::config`                          | v1.1.0 |

### `Config`

| Item                              | Since |
|-----------------------------------|-------|
| field `max_alloc`, `Config::new` (`const fn`), `Config::with_max_alloc` (`const fn`) | v0.2.0 |
| field `max_depth`, `Config::with_max_depth` (`const fn`) | v1.1.0 |
| field `canonical`, `Config::with_canonical` (`const fn`) | v1.1.0 |

### `SerialError` methods

| Method                            | Since |
|-----------------------------------|-------|
| `SerialError::custom`             | v1.1.0 |

### Re-exported derive macros (feature `derive`)

| Path                              | Since |
|-----------------------------------|-------|
| `#[derive(pack_io::Serialize)]`   | v0.4.0 |
| `#[derive(pack_io::Deserialize)]` | v0.4.0 |
| `#[derive(pack_io::DeserializeView)]` | v0.4.0 (lifetime-less types: v1.1.0) |

### Derive attributes (feature `derive`)

| Attribute                                  | Where | Since |
|--------------------------------------------|-------|-------|
| `#[pack_io(version = N)]`                  | struct | v0.5.0 |
| `#[pack_io(since = N)]`                    | field of a versioned struct | v0.5.0 |
| `#[pack_io(deprecated = N)]`               | field of a versioned struct | v0.5.0 |
| `#[pack_io(crate = "path")]`               | struct / enum | v1.1.0 |
| `#[pack_io(skip)]`                         | field | v1.1.0 |
| `#[pack_io(default)]` / `#[pack_io(default = "path")]` | field | v1.1.0 |
| `#[pack_io(with = "module")]`              | field | v1.1.0 |
| `#[pack_io(index = N)]`                    | enum variant | v1.1.0 |

### `SerialError` variants

| Variant                                          | Since |
|--------------------------------------------------|-------|
| `UnexpectedEof { needed, remaining }`            | v0.2.0 |
| `InvalidLength { declared, remaining }`          | v0.2.0 |
| `VarintOverflow`                                 | v0.2.0 |
| `IntegerOutOfRange`                              | v0.2.0 |
| `InvalidBool { byte }`                           | v0.2.0 |
| `InvalidUtf8`                                    | v0.2.0 |
| `InvalidTag { kind, tag }`                       | v0.2.0 |
| `TrailingBytes { remaining }`                    | v0.2.0 |
| `UnknownVariant { kind, index }`                 | v0.4.0 |
| `Io { kind, message }` *(feature `std`)*         | v0.3.0 |
| `DepthLimitExceeded { max_depth }`               | v1.1.0 |
| `NonCanonical { reason }`                        | v1.1.0 |
| `InvalidVersion { version }`                     | v1.1.0 |
| `InvalidValue { kind }`                          | v1.1.0 |
| `InvalidConfig { reason }`                       | v1.1.0 |
| `Custom { message }`                             | v1.1.0 |

`SerialError` is `#[non_exhaustive]`, so callers must include a wildcard
`match` arm. New variants may be added in backwards-compatible MINOR
releases.

### Feature flags

| Feature  | Default | Since |
|----------|---------|-------|
| `std`    | yes     | v0.1.0 |
| `derive` | no      | v0.4.0 |
| `schema` | no      | v0.5.0 (alias for `derive`) |
| `serde`  | no      | v0.1.0 (adapter implemented in v1.1.0) |
| `bytes`  | no      | v1.1.0 |

All feature flags are **additive**. Enabling a feature never removes or
changes existing behaviour; disabling a feature never breaks code that
did not opt into it.

### Out of the public surface (intentional)

- The `pack-io-derive` crate is an implementation detail. Depending on
  it directly is unsupported — the version pin is exact (`=X.Y.Z`)
  precisely to prevent users from getting a different revision than
  the parent crate expects. The two crates are always released together.
- `pack_io::__private` exists only for derive-generated code. It is
  hidden from the docs and may change in any release.
- The `varint` module is `pub(crate)`. The wire-format spec at
  [`docs/WIRE_FORMAT.md`](./WIRE_FORMAT.md) defines the LEB128 layout
  normatively; `pack_io::varint_len` is the one public helper.
- Test-only and benchmark-only items in `tests/` and `benches/` are
  not part of the public surface.

---

## Overview

`pack-io` exposes a compact binary codec in three layers:

- **Tier 1** — the [`encode`](#encode) / [`decode`](#decode--decode_with_config)
  free functions. One line each direction, no setup, no type parameters
  the caller has to name beyond the target type.
- **Tier 2** — concrete encoder / decoder pairs:
  - **2a, in-memory**: [`Encoder`](#encoder) + [`Decoder`](#decoder),
    backed by `Vec<u8>` / `&[u8]`. Best when the whole payload already
    lives in memory.
  - **2b, streaming**: [`IoEncoder<W>`](#ioencoder) +
    [`IoDecoder<R>`](#iodecoder), wrapping any `std::io::Write` / `Read`.
    Plus [`encode_into`](#encode_into) /
    [`decode_from`](#decode_from--decode_from_with_config) convenience
    helpers. Gated on the default `std` feature.
- **Tier 3** — the [`Serialize`](#serialize--deserialize-value-traits) and
  [`Deserialize`](#serialize--deserialize-value-traits) traits implemented
  on your own types, by hand or with the derive macros. Both are generic
  over the [`Encode`](#encode--decode-behaviour-traits) and
  [`Decode`](#encode--decode-behaviour-traits) behaviour traits, so a
  single impl works through every concrete encoder / decoder the crate
  ships.

Properties that hold across every release:

- The encode path never allocates beyond the output buffer, except for
  maps and sets, which encode their entries into one scratch buffer to
  sort them.
- Encoding is **deterministic** — the same value always produces the same
  bytes, regardless of platform, insertion order, hash randomisation, or
  build flags.
- Decoding is **safe under untrusted input** — see the next section.

The normative byte-level spec lives in
[`docs/WIRE_FORMAT.md`](./WIRE_FORMAT.md).

---

## Decoding untrusted input

Every decoder in the crate enforces these limits on any byte sequence:

| Guarantee | Mechanism |
|---|---|
| No panic, no read past the input | every read is bounds-checked and returns `Result` |
| No single value larger than `max_alloc` | length prefixes and element counts are checked against [`Config::max_alloc`](#config) (default 1 GiB) |
| No allocation the input cannot back | in-memory decoders compare declared lengths with the remaining input before allocating; streaming decoders grow buffers in bounded steps as bytes arrive; collection pre-allocation is capped by the remaining input and 64 KiB |
| No stack overflow on nested input | each derived type counts one level against [`Config::max_depth`](#config) (default 128) |
| Versioned bodies obey the caller's limits | the body is decoded under the same `Config` and nesting depth as the enclosing value |
| Optional: one byte string per value | [`Config::canonical`](#canonical-decoding) rejects every non-canonical encoding |

Decode time is linear in the input for every built-in type, with one
exception: a collection of zero-sized elements (`Vec<()>`, a unit struct)
declares its count without any bytes to back it, so its decode loop runs
`count` times. `max_alloc` caps that count per collection; lower it for
untrusted input that may contain such types.

For untrusted input, tighten the defaults to what your protocol needs:

```rust
use pack_io::{Config, decode_with_config};

let cfg = Config::new().with_max_alloc(64 * 1024).with_max_depth(32);
let bytes = pack_io::encode(&vec![1u32, 2, 3]).unwrap();
let v: Vec<u32> = decode_with_config(&bytes, cfg).unwrap();
assert_eq!(v, [1, 2, 3]);
```

---

## Tier 1 — the lazy path

### `encode`

```rust,ignore
pub fn encode<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>>;
```

Encode `value` into a freshly allocated `Vec<u8>`. The buffer starts with
a 512-byte capacity so that typical messages encode with one allocation;
for a value you will store for a long time, size the buffer exactly with
[`Serialize::encoded_len`](#serialize--deserialize-value-traits) instead
(see the example there).

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

### `decode` / `decode_with_config`

```rust,ignore
pub fn decode<T: Deserialize>(bytes: &[u8]) -> Result<T>;
pub fn decode_with_config<T: Deserialize>(bytes: &[u8], config: Config) -> Result<T>;
```

Decode a value of type `T` from `bytes`, requiring the input to be fully
consumed. Trailing input is rejected with
[`SerialError::TrailingBytes`](#serialerror). `decode` uses
[`Config::default()`](#config); `decode_with_config` validates and uses
the given config.

**Example — strict decode rejects trailing bytes:**

```rust
let mut bytes = pack_io::encode(&7_u8).unwrap();
bytes.push(0xff);
let err = pack_io::decode::<u8>(&bytes).unwrap_err();
assert!(matches!(err, pack_io::SerialError::TrailingBytes { remaining: 1 }));
```

**Example — a tight cap for one call:**

```rust
use pack_io::{Config, SerialError, decode_with_config, encode};

let bytes = encode(&"x".repeat(100)).unwrap();
let cfg = Config::new().with_max_alloc(16);
let err = decode_with_config::<String>(&bytes, cfg).unwrap_err();
assert!(matches!(err, SerialError::InvalidLength { declared: 100, .. }));
```

---

## Tier 2a — in-memory codec

### `Encoder`

In-memory encoder. Writes into an owned `Vec<u8>`; the buffer can be
swapped in / out so a single allocation serves many encodes.

| Constructor                         | Purpose                                          |
|-------------------------------------|--------------------------------------------------|
| `Encoder::new()`                    | Empty buffer.                                    |
| `Encoder::with_capacity(n)`         | Pre-sized buffer (pair with `encoded_len`).      |
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
| `Decoder::new(bytes)`                    | Default config (1 GiB `max_alloc`, depth 128, lenient). |
| `Decoder::with_config(bytes, cfg)`       | Validates `cfg` (see [`Config`](#config)).           |

| Method         | Returns          | Purpose                                  |
|----------------|------------------|------------------------------------------|
| `read::<T>()`  | `Result<T>`      | Decode one value of `T`, advance cursor. |
| `read_borrowed(n)` | `Result<&'a [u8]>` | Borrow the next `n` bytes (no prefix). |
| `position()`   | `usize`          | Bytes consumed so far.                   |
| `remaining()`  | `usize`          | Bytes left in the input.                 |
| `is_empty()`   | `bool`           | `remaining() == 0`.                      |
| `config()`     | `Config`         | The limits this decoder enforces.        |

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
    pub max_alloc: usize,  // default 1 GiB (Config::DEFAULT_MAX_ALLOC)
    pub max_depth: usize,  // default 128   (Config::DEFAULT_MAX_DEPTH)
    pub canonical: bool,   // default false
}
```

`#[non_exhaustive]` so MINOR releases can add knobs without breaking
downstream code. Build it with the `const fn` builders:

| Constructor / builder                | Purpose                                            |
|--------------------------------------|----------------------------------------------------|
| `Config::new()` / `Config::default()`| The defaults above.                                |
| `with_max_alloc(n)`                  | Cap on any single length-prefixed value.           |
| `with_max_depth(n)`                  | Cap on nesting of derived types.                   |
| `with_canonical(bool)`               | Strict canonical decoding (see [Canonical decoding](#canonical-decoding)). |

Validation happens once, when a decoder is built: `max_alloc == 0` is
rejected with `InvalidLength { declared: 0, .. }` (as in 1.0) and
`max_depth == 0` with `InvalidConfig`. `IoDecoder` additionally rejects
`canonical = true` (see [`IoDecoder`](#iodecoder)).

`max_depth` counts derived values: each `#[derive(Deserialize)]` /
`#[derive(DeserializeView)]` struct or enum is one level while its fields
decode. Built-in containers do not count (they cannot recurse without a
user type). The default of 128 is the `serde_json` default; measured on
Windows x86_64 (Rust 1.95, debug build), a five-field recursive struct
uses about 3 KiB of stack per level, so 128 levels fit in a 1 MiB thread
stack with room to spare. Raise it only for data you trust to be that
deep, and lower it for very large structs decoded on small stacks.

**Example — limits for untrusted input:**

```rust
let cfg = pack_io::Config::new()
    .with_max_alloc(16 * 1024)
    .with_max_depth(16);
assert_eq!((cfg.max_alloc, cfg.max_depth), (16 * 1024, 16));
```

---

## Tier 2b — streaming codec

The streaming pair (`std` feature, default-on) wraps any `std::io::Write`
or `Read`. Bytes are written through to the underlying sink as they are
produced — useful for sockets, files, and pipes where buffering the entire
payload in memory is undesirable. Both flavours surface I/O failure
through [`SerialError::Io`](#serialerror). Neither buffers internally:
wrap raw sockets and files in `BufWriter` / `BufReader`.

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
| `IoDecoder::new(reader)`                 | Default `Config`.                                    |
| `IoDecoder::with_config(reader, cfg)`    | Validates `cfg`; rejects `canonical = true`.         |

| Method               | Returns       | Purpose                                  |
|----------------------|---------------|------------------------------------------|
| `read::<T>()`        | `Result<T>`   | Decode one value of `T` from the reader. |
| `reader()`           | `&R`          | Borrow the underlying reader.            |
| `into_inner()`       | `R`           | Consume the decoder, return the reader.  |
| `config()`           | `Config`      | The limits this decoder enforces.        |

A streaming decoder cannot know how much input is left, so it never
sizes a buffer from a declared length: it reads in chunks (64 KiB, then
doubling) as the bytes arrive. A peer that declares a 1 GiB string and
then stalls costs one chunk, not 1 GiB. Canonical mode needs the raw
input to compare map keys, which a stream does not keep; read the frame
into memory and use [`decode_with_config`](#decode--decode_with_config)
(or [`decode_from_with_config`](#decode_from--decode_from_with_config)).

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

### `decode_from` / `decode_from_with_config`

```rust,ignore
pub fn decode_from<T: Deserialize, R: Read>(reader: &mut R) -> Result<T>;
pub fn decode_from_with_config<T: Deserialize, R: Read>(reader: &mut R, config: Config) -> Result<T>;
```

Read the rest of `reader` into memory — **at most `max_alloc` bytes** —
and decode it as a single value of type `T`. A longer stream is cut off
and rejected with [`SerialError::InvalidLength`](#serialerror) (1.0 read
without limit). Trailing bytes after the value are rejected with
[`SerialError::TrailingBytes`](#serialerror). Because the frame ends up in
memory, every `Config` knob applies, canonical mode included.

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
    // Provided; every one has a correct, bounded default:
    fn remaining_hint(&self) -> Option<usize>;
    fn config(&self) -> Config;
    fn depth(&self) -> usize;
    fn enter_nested(&mut self) -> Result<()>;
    fn leave_nested(&mut self);
    fn consumed(&self) -> Option<&[u8]>;
    fn read_byte_cow(&mut self, len: usize) -> Result<Cow<'_, [u8]>>;
    fn read_varint_u64(&mut self) -> Result<u64>;
    fn read_varint_u128(&mut self) -> Result<u128>;
    fn read_length_prefixed(&mut self) -> Result<Vec<u8>>;
}
```

The behavioural seam — implemented by every concrete encoder / decoder in
the crate (and, with the `bytes` feature, `Encode` by `BytesMut`). User
code rarely implements these directly; they exist so that
[`Serialize`](#serialize--deserialize-value-traits) and
[`Deserialize`](#serialize--deserialize-value-traits) impls can be written
generically and work through both the in-memory and streaming codec
flavours unchanged.

A custom `Decode` needs only the three required methods. It should also
override `remaining_hint` when it knows how much input is left (so
collection pre-allocation is bounded by it), the depth methods to
enforce `max_depth`, and `consumed` if it keeps its input in memory (so
canonical mode can check map key order).

Hand-written `Deserialize` impls for **recursive** types should call
`enter_nested` / `leave_nested` around their body (derived impls do):

```rust
use pack_io::{Decode, Deserialize, Result};

struct Tree(Vec<Tree>);

impl Deserialize for Tree {
    fn deserialize<D: Decode + ?Sized>(dec: &mut D) -> Result<Self> {
        dec.enter_nested()?;
        let kids = Vec::<Tree>::deserialize(dec);
        dec.leave_nested(); // on the error path too
        Ok(Tree(kids?))
    }
}

let hostile = vec![1u8; 10_000]; // 10,000 levels of `Tree([Tree([...])])`
assert!(matches!(
    pack_io::decode::<Tree>(&hostile),
    Err(pack_io::SerialError::DepthLimitExceeded { .. })
));
```

### `Serialize` / `Deserialize` (value traits)

```rust,ignore
pub trait Serialize {
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()>;
    fn serialize_slice<E: Encode + ?Sized>(slice: &[Self], encoder: &mut E) -> Result<()> where Self: Sized;
    fn encoded_len(&self) -> Result<usize>;
    fn encoded_len_slice(slice: &[Self]) -> Result<usize> where Self: Sized;
}

pub trait Deserialize: Sized {
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self>;
    fn deserialize_many<D: Decode + ?Sized>(decoder: &mut D, count: usize) -> Result<Vec<Self>>;
    fn deserialize_array<D: Decode + ?Sized, const N: usize>(decoder: &mut D) -> Result<[Self; N]>;
}
```

**Contract:**

- `serialize` MUST be deterministic: equal values produce equal bytes.
- `serialize` appends to the encoder's buffer / sink; it does not clear
  what's already there.
- `encoded_len` MUST return exactly the number of bytes `serialize`
  writes. The default computes it by running `serialize` against a
  counting sink; the built-in impls and the derive use direct arithmetic.
  Versioned structs write their body length from it, so an override that
  miscounts corrupts the frame.
- `deserialize` consumes exactly the bytes a corresponding `serialize`
  would have produced.
- On malformed input, `deserialize` MUST return an error and MUST NOT
  panic, allocate more than the input can back, or read past the
  underlying source.

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
assert_eq!(p.encoded_len().unwrap(), bytes.len());
```

**Example — exactly-sized output for long-lived storage:**

```rust
use pack_io::{Encoder, Serialize};

let value = (7_u64, String::from("stored key"));
let mut enc = Encoder::with_capacity(value.encoded_len().unwrap());
value.serialize(&mut enc).unwrap();
let bytes = enc.into_inner();
assert_eq!(bytes.len(), bytes.capacity());
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

The derive macros (feature `derive`, default off) write `Serialize` and
`Deserialize` impls for any struct (named, tuple, unit) and any non-empty
enum (any variant shape), generic over type parameters. The generated
code uses only `::core` and the `pack_io` crate path, so it compiles in
`#![no_std]` crates.

Field order in the source code is the encoded byte order. Enums are
encoded as `varint(variant_index) ++ fields`, where `variant_index` is
the variant's source-declaration position starting at `0` unless an
explicit [`index`](#derive-attributes) says otherwise. **Append new
variants to the end** of an enum declaration (or give variants explicit
indices) to preserve wire-format compatibility.

Unknown variant indices on decode surface as
[`SerialError::UnknownVariant`](#serialerror) carrying the real index.
Every derived `Deserialize` counts one level against `max_depth`.

The macros are re-exported at `pack_io::{Serialize, Deserialize,
DeserializeView}` — the underlying [`pack-io-derive`](https://crates.io/crates/pack-io-derive)
proc-macro crate is an implementation detail.

### Derive attributes

Every attribute lives under `#[pack_io(...)]`. Anything unknown,
duplicated, or used in the wrong place is a compile error pointing at
the attribute (1.0 silently ignored attributes on enum fields and
variants).

| Attribute | Where | Effect |
|---|---|---|
| `version = N` | struct | versioned frame, see [Schema evolution](#schema-evolution); `N >= 1` |
| `since = N` | field of a versioned struct | field added at version `N`; `N` must not exceed the struct's `version` |
| `deprecated = N` | field of a versioned struct | field removed from the wire at version `N`; `N > since` |
| `skip` | any field | never encoded; decoded as its default |
| `default` / `default = "path"` | any field with `skip`, `since` or `deprecated` | value used when the field is absent: `Default::default()` or `path()` |
| `with = "module"` | any field | encode with `module::serialize(&field, encoder)`, decode with `module::deserialize(decoder)`, and in views `module::deserialize_view(decoder)` |
| `crate = "path"` | struct / enum | path to `pack_io` in generated code (default `::pack_io`) — for crates that re-export pack-io |
| `index = N` | enum variant | explicit wire tag; later variants without one continue from `N + 1`; duplicates are rejected |

```rust
use pack_io::{Decode, Deserialize, Encode, Result, Serialize};

/// Store a `u32` as four little-endian bytes instead of a varint.
mod fixed_u32 {
    use pack_io::{Decode, Encode, Result};
    pub fn serialize<E: Encode + ?Sized>(v: &u32, e: &mut E) -> Result<()> {
        e.write_bytes(&v.to_le_bytes())
    }
    pub fn deserialize<D: Decode + ?Sized>(d: &mut D) -> Result<u32> {
        let mut b = [0u8; 4];
        d.read_into(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }
}

fn unknown() -> String { "unknown".into() }

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Header {
    #[pack_io(with = "fixed_u32")]
    magic: u32,
    #[pack_io(skip, default = "unknown")]
    source: String, // local only, never on the wire
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Op {
    #[pack_io(index = 10)]
    Get,
    Put, // 11
}

let h = Header { magic: 0xCAFE_BABE, source: "socket".into() };
let bytes = pack_io::encode(&(h, Op::Put)).unwrap();
assert_eq!(bytes, [0xBE, 0xBA, 0xFE, 0xCA, 11]);
let (back, op): (Header, Op) = pack_io::decode(&bytes).unwrap();
assert_eq!((back.source.as_str(), op), ("unknown", Op::Put));
```

`Box<T>` has no blanket impl in 1.x (see
[Supported types](#supported-types)); recursive types that need a box
can use `with`:

```rust
use pack_io::{Decode, Deserialize, Encode, Result, Serialize};

mod boxed {
    use pack_io::{Decode, Deserialize, Encode, Result, Serialize};
    pub fn serialize<T: Serialize, E: Encode + ?Sized>(v: &Option<Box<T>>, e: &mut E) -> Result<()> {
        v.as_deref().serialize(e)
    }
    pub fn deserialize<T: Deserialize, D: Decode + ?Sized>(d: &mut D) -> Result<Option<Box<T>>> {
        Ok(Option::<T>::deserialize(d)?.map(Box::new))
    }
}

#[derive(Serialize, Deserialize)]
struct List {
    value: u8,
    #[pack_io(with = "boxed")]
    next: Option<Box<List>>,
}

let list = List { value: 1, next: Some(Box::new(List { value: 2, next: None })) };
assert_eq!(pack_io::encode(&list).unwrap(), [1, 1, 2, 0]);
```

---

## Zero-copy decode

The owning [`Deserialize`](#serialize--deserialize-value-traits) surface
allocates `String`s and `Vec<u8>`s during decode. The zero-copy
[`DeserializeView`](#deserializeview) surface returns `&'a str` / `&'a [u8]`
that borrow directly from the input slice — no per-field allocation. Both
surfaces use the **same on-wire format** and enforce the same `Config`
limits; choose the surface that matches the lifetime relationship the
caller has with the source bytes.

On a representative borrow-heavy record, the view decode runs about
**2.3×** faster than the owning decode on the measurement machine (25 ns
against 57 ns; see
[`docs/PERFORMANCE_BASELINE.md`](./PERFORMANCE_BASELINE.md)). Reproduce with
`cargo bench --bench codec_bench --features derive`.

### `DeserializeView`

```rust,ignore
pub trait DeserializeView<'a>: Sized {
    fn deserialize_view(decoder: &mut Decoder<'a>) -> Result<Self>;
    fn deserialize_view_many(decoder: &mut Decoder<'a>, count: usize) -> Result<Vec<Self>>;
    fn deserialize_view_array<const N: usize>(decoder: &mut Decoder<'a>) -> Result<[Self; N]>;
}
```

The borrowed counterpart to [`Deserialize`](#serialize--deserialize-value-traits).
`'a` is the lifetime of the input buffer; the borrow checker guarantees
the decoded value cannot outlive its source.

Built-in implementors:

- `&'a str`, `&'a [u8]`, `Cow<'a, str>`, `Cow<'a, [u8]>` (as
  `Cow::Borrowed`) — the zero-copy types.
- Every primitive (`u8` … `u128`, `i8` … `i128`, `usize`, `isize`,
  `bool`, `char`, `f32`, `f64`, `()`), `NonZero*`, `Duration`,
  `PhantomData<T>`, and owned text (`String`, `Box<str>`, `Rc<str>`,
  `Arc<str>`) — these reduce to `Deserialize`.
- `Option<T>`, `Result<T, E>`, tuples (arity 1–12), fixed arrays `[T; N]`.
- `Vec<T>`, `VecDeque<T>`, `BTreeMap<K, V>`, `BTreeSet<T>`,
  `HashMap<K, V>` *(std)*, `HashSet<T>` *(std)* — container allocated,
  elements may still borrow.

### `decode_view` / `decode_view_with_config`

```rust,ignore
pub fn decode_view<'a, T: DeserializeView<'a>>(bytes: &'a [u8]) -> Result<T>;
pub fn decode_view_with_config<'a, T: DeserializeView<'a>>(bytes: &'a [u8], config: Config) -> Result<T>;
```

Tier-1 zero-copy entry points — the counterparts of
[`decode` / `decode_with_config`](#decode--decode_with_config). Strict:
trailing bytes are rejected with [`SerialError::TrailingBytes`](#serialerror).

**Example:**

```rust
let bytes = pack_io::encode(&"hello").unwrap();
let view: &str = pack_io::decode_view(&bytes).unwrap();
assert_eq!(view, "hello"); // borrowed from `bytes`
```

<a id="decoder-borrowed-reads"></a>
### `Decoder::read_length_prefixed_borrowed` / `read_borrowed`

```rust,ignore
impl<'a> Decoder<'a> {
    pub fn read_length_prefixed_borrowed(&mut self) -> Result<&'a [u8]>;
    pub fn read_borrowed(&mut self, len: usize) -> Result<&'a [u8]>;
}
```

`read_length_prefixed_borrowed` reads a varint length prefix, validates
it against [`Config::max_alloc`](#config) and the remaining input, then
returns a borrowed slice over the next `length` bytes; it powers the
`&'a str` / `&'a [u8]` view impls. `read_borrowed` borrows a fixed number
of bytes with no prefix, for fields whose length the caller already
knows. Neither exists on [`IoDecoder<R>`](#iodecoder) — a stream has no
buffer to borrow from.

### `#[derive(DeserializeView)]`

```rust
use pack_io::{DeserializeView, Serialize, decode_view, encode};

#[derive(Serialize, DeserializeView)]
struct Header { id: u64 }                 // no lifetime: nests in views

#[derive(Serialize, DeserializeView)]
#[pack_io(version = 2)]
struct ViewMsg<'a> {
    header: Header,
    text: &'a str,
    #[pack_io(since = 2)]
    payload: &'a [u8],
}

let bytes = encode(&ViewMsg { header: Header { id: 1 }, text: "hi", payload: b"x" }).unwrap();
let view: ViewMsg<'_> = decode_view(&bytes).unwrap();
assert_eq!((view.header.id, view.text, view.payload), (1, "hi", &b"x"[..]));
```

The derive (feature `derive`) writes a [`DeserializeView`](#deserializeview)
impl for any struct or enum with **at most one** lifetime parameter:

- One lifetime `'a`: `impl DeserializeView<'a>`; borrowed fields point
  into the input.
- No lifetime: an impl for every lifetime, so owned helper types
  (headers, ids, enums of scalars) can be nested inside borrowing views
  (1.0 rejected these).

Every derive attribute works on views too, including
`#[pack_io(version = N)]`: the view decoder reads the same versioned
frame as the owning one and borrows the body instead of copying it (1.0
ignored the attribute here and mis-parsed versioned payloads). `with`
modules must provide `deserialize_view(&mut Decoder<'a>)`.

---

## Schema evolution

A struct tagged with `#[pack_io(version = N)]` encodes as a length-framed
body (`varint(version) ++ varint(body_len) ++ body`) so that old and new
revisions interoperate as long as fields are **appended**. Old decoders
skip trailing body bytes they don't recognise; new decoders
default-construct fields that older payloads didn't include.

The attributes need only the `derive` feature (`schema` is an alias kept
for compatibility). Full normative spec:
[`docs/WIRE_FORMAT.md §3.8`](./WIRE_FORMAT.md#38-versioned-structs).

### `#[pack_io(version)]`

Type-level attribute marking the struct as versioned. `N` is a positive
`u32`; `0` is rejected at compile time, and a payload carrying version
`0` is rejected at decode time with
[`SerialError::InvalidVersion`](#serialerror) (1.0 accepted it, against
the spec).

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

The body length is validated against `max_alloc` and the remaining input
before anything is read, and the body is decoded under the caller's
`Config` and nesting depth (1.0 decoded it with a fresh default config).
In-memory decoding borrows the body; nothing is copied.

### `#[pack_io(since)]`

Field-level attribute marking the field as **added** at version `N`
(default `1`). `N` must not exceed the struct's own `version` — 1.0
accepted that and silently never encoded the field; it is now a compile
error. Decoders reading payloads from versions before `N` fill the field
with `Default::default()` or the `#[pack_io(default = "path")]` function.

### `#[pack_io(deprecated)]`

Field-level attribute marking the field as **removed from the wire** at
version `N`. The field stays in the struct declaration; encoders at
version `>= N` omit it and decoders reading payloads at version `>= N`
default it. `N` must be greater than `since`.

Removal takes bytes out of the *middle* of the body, so it is only safe
once every reader knows about it. A reader built before the deprecation
cannot tell the field is gone and decodes the following field's bytes
into it. Roll it out **reader-first**:

1. Ship readers at version `N - 1` that already declare
   `#[pack_io(deprecated = N)]` on the field. They still write the
   field, and they read version-`N` payloads correctly.
2. Once every reader runs that code, bump writers to `version = N`.

If you cannot control reader rollout (data at rest read by old
binaries, third-party consumers), do not deprecate: keep the field,
stop using it, and let it carry its default value.

### `peek_version`

```rust,ignore
pub fn peek_version(bytes: &[u8]) -> Result<u32>;
```

Read only the leading `varint(version)` of a versioned payload without
consuming the buffer or decoding the body. Useful when a single
transport carries multiple revisions and the dispatcher needs to pick a
target type at runtime.

```rust
# #[cfg(feature = "derive")] {
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

## Canonical decoding

The encoder produces exactly one byte string per value. By default the
decoder is lenient — as the wire spec allows — and also accepts a few
other byte strings for the same value. Where the bytes are hashed,
signed, deduplicated or used as a content address, turn on
`Config::with_canonical(true)`: the decoder then accepts only the
encoder's own output and rejects everything else with
[`SerialError::NonCanonical { reason }`](#serialerror):

| `reason` | Rejected input |
|---|---|
| `"varint"` | a multi-byte varint whose last byte is `0x00` (e.g. `[0x80, 0x00]` for `0`) |
| `"map key order"` | map entries not in strictly increasing encoded-key order (covers duplicates) |
| `"set element order"` | set elements not strictly increasing (covers duplicates) |
| `"versioned body"` | unread bytes inside a versioned body whose version is not newer than the reader's type |

Bytes inside a body *newer* than the reader's type are unknown fields
and are still skipped; canonical injectivity is promised for payloads at
the reader's own version.

```rust
use std::collections::BTreeMap;
use pack_io::{Config, SerialError, decode, decode_with_config};

// {2: 20, 1: 10, 2: 99}: unsorted, with a duplicate key.
let bytes = [3, 2, 20, 1, 10, 2, 99];
let lenient: BTreeMap<u32, u32> = decode(&bytes).unwrap();
assert_eq!(lenient.len(), 2);

let strict = Config::new().with_canonical(true);
assert_eq!(
    decode_with_config::<BTreeMap<u32, u32>>(&bytes, strict),
    Err(SerialError::NonCanonical { reason: "map key order" }),
);
```

Canonical mode is supported by the in-memory [`Decoder`](#decoder) and
everything built on it (`decode_with_config`, `decode_view_with_config`,
`decode_from_with_config`, the serde adapter). It costs nothing on
inputs without maps; map and set decoding compares each key with the
previous one in place (no copies).

---

## serde adapter (feature `serde`)

`pack_io::serde` puts any `serde::Serialize` / `serde::Deserialize` type
on the pack-io wire. A serde-derived type encodes to **the same bytes**
as the equivalent `#[derive(pack_io::Serialize)]` type: structs and
tuples are their fields concatenated, enums are `varint(index) ++ fields`,
sequences and strings are length-prefixed, maps are written in canonical
key order.

| Item | Use |
|---|---|
| `to_vec(&v)` | encode into a fresh `Vec<u8>` |
| `to_encoder(&v, &mut enc)` | encode into any `Encode` sink |
| `from_slice(&bytes)` / `from_slice_with_config(&bytes, cfg)` | decode one value from a whole buffer; `&str` / `&[u8]` fields borrow |
| `from_decoder(&mut decoder)` | decode the next value from a `Decoder` (borrowing) |
| `from_decode(&mut any_decoder)` | decode the next value from any `Decode`, e.g. `IoDecoder` (owning) |
| `compat::{serialize, deserialize, deserialize_view}` | `#[pack_io(with = "pack_io::serde::compat")]` for one serde-typed field |
| `Compat<T>` | wrapper that gives a serde type pack-io's traits |

```rust
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
enum Shape {
    Circle { radius: u32 },
    Rect(u32, u32),
}

let bytes = pack_io::serde::to_vec(&Shape::Rect(3, 4)).unwrap();
assert_eq!(bytes, [1, 3, 4]);
let back: Shape = pack_io::serde::from_slice(&bytes).unwrap();
assert_eq!(back, Shape::Rect(3, 4));
```

Limits: pack-io is not self-describing, so `deserialize_any` (untagged
and internally tagged enums, `#[serde(flatten)]`) and
`deserialize_ignored_any` return `SerialError::Custom`. serde treats sets
as sequences, so a `HashSet` serialised through serde is written in
iteration order — use `BTreeSet` or the native derive where set bytes
must be deterministic. Decoding enforces the same `max_alloc`,
`max_depth` and canonical-mode rules as the native path.

---

## bytes integration (feature `bytes`)

- `bytes::Bytes` and `bytes::BytesMut` implement `Serialize` /
  `Deserialize` with the same wire shape as `Vec<u8>`.
- `BytesMut` implements `Encode`, so a value encodes straight into a
  tokio codec's output buffer.
- Zero-copy from a `Bytes` frame: decode a view and turn each borrowed
  `&[u8]` into a reference-counted slice with `Bytes::slice_ref`.

```rust,ignore
use bytes::{Bytes, BytesMut};
use pack_io::{Serialize, decode_view};

let mut buf = BytesMut::new();
(7_u32, b"payload".as_slice()).serialize(&mut buf).unwrap();
let frame: Bytes = buf.freeze();

let (id, body): (u32, &[u8]) = decode_view(&frame).unwrap();
let body: Bytes = frame.slice_ref(body); // no copy
```

---

## Errors

### `SerialError`

```rust,ignore
#[non_exhaustive]
pub enum SerialError {
    UnexpectedEof      { needed: usize, remaining: usize },
    InvalidLength      { declared: u64, remaining: usize },
    VarintOverflow,
    IntegerOutOfRange,
    InvalidBool        { byte: u8 },
    InvalidUtf8,
    InvalidTag         { kind: &'static str, tag: u8 },
    UnknownVariant     { kind: &'static str, index: u64 },
    TrailingBytes      { remaining: usize },
    #[cfg(feature = "std")]
    Io                 { kind: std::io::ErrorKind, message: String },
    DepthLimitExceeded { max_depth: usize },
    NonCanonical       { reason: &'static str },
    InvalidVersion     { version: u64 },
    InvalidValue       { kind: &'static str },
    InvalidConfig      { reason: &'static str },
    Custom             { message: Cow<'static, str> },
}
```

`#[non_exhaustive]` so MINOR releases can add variants without breaking
downstream `match` arms.

| Variant | When | Caller action |
|---------|------|---------------|
| `UnexpectedEof` | Decoder needed more bytes than were available. | Treat as truncated transport. |
| `InvalidLength` | A length prefix exceeded the buffer or `max_alloc`; `decode_from` read more than `max_alloc`. | Reject the message. |
| `VarintOverflow` | A LEB128 varint exceeded its target width. | Reject. |
| `IntegerOutOfRange` | A decoded integer did not fit in the requested narrower target. | Reject; producer / consumer width mismatch. |
| `InvalidBool` | A boolean byte was neither `0x00` nor `0x01`. | Reject. |
| `InvalidUtf8` | A length-prefixed byte run was not valid UTF-8. | Reject. |
| `InvalidTag` | An `Option` / `Result` tag was outside `0x00` / `0x01`. | Reject. |
| `UnknownVariant` | An enum tag matched no variant of the target type; `index` is the tag read. | Reject; producer / consumer are at incompatible enum revisions. |
| `TrailingBytes` | A whole-buffer decode left bytes unread. | Either producer wrong, or use [`Decoder`](#decoder) for multi-value streams. |
| `Io` *(std-only)* | Underlying `Read` / `Write` failed. | Inspect `kind` and `message`; surface to transport. |
| `DepthLimitExceeded` | Nesting exceeded `max_depth`. | Treat as hostile, or raise the limit for trusted deep data. |
| `NonCanonical` | Canonical mode saw a non-canonical encoding. | Reject; the bytes are not the canonical form of their value. |
| `InvalidVersion` | A versioned payload carried the reserved version `0`. | Reject as corrupt. |
| `InvalidValue` | The value violates its type's invariant (`char`, `NonZero*`, `Duration`). | Reject. |
| `InvalidConfig` | A `Config` knob was rejected when building a decoder. | Fix the configuration. |
| `Custom` | Raised by user code, a `with` module, or a serde type. | Depends on the source. |

`SerialError` implements `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`,
and (under the default `std` feature) `std::error::Error`; with the
`serde` feature it also implements `serde::ser::Error` and
`serde::de::Error`. Error messages never echo the offending bytes back to
the caller — safe to log without input sanitisation.
`SerialError::custom(message)` builds a `Custom` from a `&'static str`
or a `String`.

### `Result<T>`

```rust,ignore
pub type Result<T> = core::result::Result<T, SerialError>;
```

Convenience alias used throughout the codec.

---

## Supported types

| Group | Types |
|-------|-------|
| Unsigned integers | `u8`, `u16`, `u32`, `u64`, `u128`, `usize`, `NonZeroU*` |
| Signed integers   | `i8`, `i16`, `i32`, `i64`, `i128`, `isize`, `NonZeroI*` |
| Floats            | `f32`, `f64` |
| Bool / char / unit| `bool`, `char`, `()`, `PhantomData<T>` |
| Strings           | `String`, `&str` (encode / view), `Box<str>`, `Rc<str>`, `Arc<str>`, `Cow<'_, str>` |
| Sequences         | `Vec<T>`, `&[T]` (encode), `Box<[T]>`, `Rc<[T]>`, `Arc<[T]>`, `VecDeque<T>`, `[T; N]` |
| Tuples            | arity 1 through 12 |
| Sums              | `Option<T>`, `Result<T, E>` |
| Maps              | `BTreeMap<K, V>`, `HashMap<K, V, S>` *(std)* |
| Sets              | `BTreeSet<T>`, `HashSet<T, S>` *(std)* |
| Time              | `core::time::Duration` |
| Pointers          | `&T` (encode), `Rc<T>`, `Arc<T>`, `Cow<'_, T>` — transparent |
| Byte buffers      | `bytes::Bytes`, `bytes::BytesMut` *(feature `bytes`)* |
| serde types       | any, through [`pack_io::serde`](#serde-adapter-feature-serde) *(feature `serde`)* |

User-defined types opt in via `#[derive(pack_io::Serialize,
pack_io::Deserialize)]` under the `derive` feature.

`Box<T>` is deliberately not implemented in 1.x. `Box` is a
`#[fundamental]` type, so a blanket `impl<T> Serialize for Box<T>` would
conflict with impls downstream crates are allowed to write today
(`impl pack_io::Serialize for Box<MyType>`), which makes adding it a
breaking change (RFC 1023). It is planned for 2.0; until then use
`Box<str>` / `Box<[T]>` (implemented), or a `with` module (see
[Derive attributes](#derive-attributes)).

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
- **Maps and sets encode in canonical key-sorted order** — sorted
  lexicographically by the encoded key bytes. A `HashMap` and a
  `BTreeMap` over the same logical data encode identically.

The format is frozen for the `1.x` line. 1.1 adds encodings for new
types (spec 1.3) and clarifies decoder obligations; every byte string a
1.0 encoder produced is produced identically by 1.1 and decodes to the
same value (checked against committed 1.0.1 golden vectors).

---

## Feature flags

| Feature  | Default | Description |
|----------|---------|-------------|
| `std`    | yes     | Standard library. Off → `no_std` (`core` + `alloc`). Enables [`std::error::Error`] on [`SerialError`](#serialerror), `HashMap` / `HashSet` integration, and the [`io`](#tier-2b--streaming-codec) module. |
| `derive` | no      | `#[derive(Serialize, Deserialize, DeserializeView)]` and every `#[pack_io(...)]` attribute. Pulls in the companion `pack-io-derive` crate. |
| `schema` | no      | Alias for `derive`, kept for compatibility. |
| `serde`  | no      | The [`pack_io::serde`](#serde-adapter-feature-serde) adapter. Adds the `serde` dependency (`alloc` features; `std` follows pack-io's `std`). |
| `bytes`  | no      | [`bytes`](#bytes-integration-feature-bytes) integration. Adds the `bytes` dependency (no default features). |

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
// VERSION mirrors Cargo.toml exactly, with no parsing.
assert_eq!(pack_io::VERSION, env!("CARGO_PKG_VERSION"));
```

### `varint_len`

```rust,ignore
pub const fn varint_len(value: u64) -> usize;
```

Bytes the LEB128 encoding of `value` occupies (1–10). Use it in
hand-written `encoded_len` overrides.

```rust
assert_eq!(pack_io::varint_len(127), 1);
assert_eq!(pack_io::varint_len(128), 2);
```

---

## Cross-platform notes

- Tier-1 supported targets: Linux (`x86_64`, `aarch64`), macOS (`x86_64`,
  Apple Silicon), Windows (`x86_64`). All three run the full CI matrix on
  every commit, on both stable and MSRV. CI also builds for a bare-metal
  `no_std` target (`thumbv7em-none-eabihf`).
- Encoding is byte-deterministic across all of them.
- `usize` and `isize` encode through `u64` and `i64` respectively, so a
  value encoded on a 64-bit machine that exceeds `usize::MAX` on a 32-bit
  consumer surfaces as [`SerialError::IntegerOutOfRange`](#serialerror) —
  no silent truncation.
- `no_std` builds rely on `core` + `alloc` only. The Tier-2b streaming
  codec ([`IoEncoder`](#ioencoder) / [`IoDecoder`](#iodecoder),
  [`encode_into`](#encode_into) /
  [`decode_from`](#decode_from--decode_from_with_config)) requires
  `std` and is gated on the `std` feature. `Arc` impls require a target
  with pointer-width atomics.
- `HashMap` / `HashSet` impls require `std` and are gated on the same
  feature.

---

## Compatibility & semver

- **The public surface listed in [§ Frozen public surface](#frozen-public-surface)
  is the `1.x` contract.** Source-breaking changes are deferred to
  `2.x`.
- **The wire format ([`docs/WIRE_FORMAT.md`](./WIRE_FORMAT.md), spec
  version 1.3) is frozen for the `1.x` line.** Any `1.x` decoder reads
  any `1.x`-or-earlier encoding. Wire-format-breaking changes are
  deferred to `2.x`.
- **The performance baseline ([`docs/PERFORMANCE_BASELINE.md`](./PERFORMANCE_BASELINE.md))
  is the contractual reference.** A change exceeding 5 % regression on
  any row blocks the merge.
- `1.x` minor releases ship bug fixes, hardening, performance
  improvements, and strictly *additive* API (new `SerialError` variants
  under the existing `#[non_exhaustive]` enum, new trait methods with
  default implementations, new built-in impls for `core` / `alloc` types).
- Security hardening may make a decoder refuse inputs it used to accept
  when those inputs violate a documented limit or the spec (1.1: depth
  beyond `max_depth`, version `0`, `decode_from` streams beyond
  `max_alloc`); each case is listed in the release notes with its
  escape hatch.
- Deprecated items remain available for at least one MAJOR after the
  `#[deprecated]` attribute is added.
