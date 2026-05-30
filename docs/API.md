# pack-io — API Reference

> Complete reference for every public item in `pack-io`, with examples.
> Format mirrors the portfolio standard ([metrics-lib API.md](https://github.com/jamesgober/metrics-lib/blob/main/docs/API.md)).
>
> **Status: pre-1.0.** This document tracks the API surface as it lands across
> the 0.x series. Sections marked _(planned)_ describe the intended surface and
> are filled in as each roadmap phase ships.

## Table of Contents

- [Overview](#overview)
- [Tier 1 — the lazy path](#tier-1--the-lazy-path)
  - [`encode`](#encode) _(planned: 0.2)_
  - [`decode`](#decode) _(planned: 0.2)_
- [Tier 2 — the configured path](#tier-2--the-configured-path)
  - [`Encoder` / `Decoder`](#encoder--decoder) _(planned: 0.3)_
- [Tier 3 — the power path](#tier-3--the-power-path)
  - [`Serialize` / `Deserialize` traits](#serialize--deserialize-traits) _(planned: 0.2)_
  - [`View` zero-copy decode](#view-zero-copy-decode) _(planned: 0.4)_
- [Wire format](#wire-format) _(spec lands at 0.3)_
- [Errors](#errors) _(planned: 0.2)_
- [Feature flags](#feature-flags)

---

## Overview

`pack-io` exposes a compact binary codec. The common case is a function call;
advanced use is an encoder/decoder for streaming; the full surface is the
`Serialize`/`Deserialize` traits and the zero-copy `View` types.

The hot path never allocates beyond the output buffer. The encoding is
deterministic — the same value always produces the same bytes. Decoding is
safe under untrusted input by default (bounded allocation, validated length
prefixes, no panics).

```rust
use pack_io::{encode, decode};

#[derive(pack_io::Serialize, pack_io::Deserialize)]
struct M { id: u64 }

let bytes = encode(&M { id: 7 })?;
let back: M = decode(&bytes)?;
```

---

## Tier 1 — the lazy path

_The one-line surface for the ~80% case. Documented in full as the 0.2
foundation release lands._

- `encode<T: Serialize>(value: &T) -> Result<Vec<u8>>` — encode a value into a
  freshly-allocated `Vec<u8>`.
- `decode<T: Deserialize>(bytes: &[u8]) -> Result<T>` — decode a value from a
  byte slice.

---

## Tier 2 — the configured path

_Streaming encoder/decoder for callers writing into existing buffers or reading
from `Read`. Documented at 0.3._

---

## Tier 3 — the power path

_The `Serialize`/`Deserialize` traits and the zero-copy `View<T>` decode
surface. Documented as the trait surface stabilises at 0.2 and zero-copy
ships at 0.4._

---

## Wire format

_Compact binary spec lands as a normative document at 0.3 when the format
freezes. Until then, the encoding is considered unstable across 0.x and may
change._

---

## Errors

_Construction-time and decode-time error type built on `error-forge`. The
encode path is infallible for sized in-memory values; decode returns
`Result<T, SerialError>`. Variants documented at 0.2._

---

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std`     | yes | Standard library. Off → `no_std`. |
| `derive`  | no  | `#[derive(Serialize, Deserialize)]` proc-macros. |
| `schema`  | no  | Schema-versioning and evolution helpers. |
| `serde`   | no  | Optional `serde` interop shims. |

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
