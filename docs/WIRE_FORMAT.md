<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>pack-io</b><br>
    <sub><sup>WIRE FORMAT — NORMATIVE SPECIFICATION</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="../CHANGELOG.md" title="Changelog"><b>CHANGELOG</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./API.md" title="API Reference"><b>API</b></a>
        <span>&nbsp;│&nbsp;</span>
        <span>WIRE FORMAT</span>
    </sup>
</div>
<br>

> **Status: stable as of v0.3.0.** This document is the normative
> specification for the pack-io wire format. It is written so that a reader
> who has never seen the source code can implement a compatible encoder or
> decoder.
>
> Within the `1.x` line the format is frozen — any `1.x` decoder reads any
> `1.x`-or-earlier encoding. Breaking changes inside the `0.x` series are
> called out prominently in [`CHANGELOG.md`](../CHANGELOG.md).

---

## 1. Conventions

- The keywords **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are used in
  the [RFC 2119](https://datatracker.ietf.org/doc/html/rfc2119) sense.
- Byte values are written in hexadecimal with a `0x` prefix.
- Bit positions are numbered with `0` being the least significant.
- All multi-byte numeric fields are **little-endian** unless stated
  otherwise.
- "Concatenated" means the byte sequences are placed back-to-back with no
  framing bytes, padding, or alignment.

---

## 2. Primitive encodings

### 2.1 Unsigned integers

| Type      | Encoding                                            |
|-----------|-----------------------------------------------------|
| `u8`      | 1 byte, value as-is.                                |
| `u16`     | LEB128 unsigned varint (1–3 bytes).                 |
| `u32`     | LEB128 unsigned varint (1–5 bytes).                 |
| `u64`     | LEB128 unsigned varint (1–10 bytes).                |
| `u128`    | LEB128 unsigned varint (1–19 bytes).                |
| `usize`   | Encoded as `u64`. Decoders MUST reject values that exceed the receiver's `usize::MAX` with **`IntegerOutOfRange`**. |

**LEB128 unsigned varint:** little-endian base-128 encoding. Each byte
carries seven bits of payload in its low seven bits and a continuation flag
in bit 7 (`1` = more bytes follow, `0` = this is the last byte).

Encoders MUST emit the shortest possible representation — a varint that
includes redundant zero high bytes is **invalid** and decoders MUST reject
it as **`VarintOverflow`** when the byte count exceeds the legal width
(10 for `u64`, 19 for `u128`).

For a `u64` varint, the tenth byte MAY only set bit 0 — bits 1–7 of the
tenth byte are required to be zero, otherwise the value would overflow
`u64`. The same restriction applies to byte 19 of a `u128` varint (only
bits 0–1 of byte 19 may be set).

### 2.2 Signed integers

| Type     | Encoding                                                  |
|----------|-----------------------------------------------------------|
| `i8`     | 1 byte, two's-complement value as-is.                     |
| `i16`    | ZigZag → LEB128 (`u16`-sized payload, 1–3 bytes).         |
| `i32`    | ZigZag → LEB128 (`u32`-sized payload, 1–5 bytes).         |
| `i64`    | ZigZag → LEB128 (`u64`-sized payload, 1–10 bytes).        |
| `i128`   | ZigZag → LEB128 (`u128`-sized payload, 1–19 bytes).       |
| `isize`  | Encoded as `i64`. Decoders MUST reject values that exceed the receiver's `isize` range with **`IntegerOutOfRange`**. |

**ZigZag mapping:** maps signed integers `0, -1, 1, -2, 2, …` to unsigned
`0, 1, 2, 3, 4, …` so the absolute magnitude determines the varint length.
For an N-bit signed value:

```
zz(n) = (n << 1) XOR (n >> (N-1))
n     = (zz >> 1) XOR -(zz & 1)
```

### 2.3 Boolean

| Type   | Encoding                                                          |
|--------|-------------------------------------------------------------------|
| `bool` | 1 byte. `0x00` = `false`, `0x01` = `true`. Any other byte is **invalid** and decoders MUST reject it as **`InvalidBool { byte }`**. |

### 2.4 Floating-point

| Type   | Encoding                                                       |
|--------|----------------------------------------------------------------|
| `f32`  | 4 bytes, IEEE 754 binary32 bit pattern, little-endian.         |
| `f64`  | 8 bytes, IEEE 754 binary64 bit pattern, little-endian.         |

The bit pattern is preserved exactly — NaN, ±Inf, subnormals, and signed
zeros all round-trip bit-for-bit. Consumers that need IEEE equality on the
decoded value MUST compare via `to_bits()` (since `NaN != NaN` by spec).

### 2.5 Unit type

| Type   | Encoding                                                       |
|--------|----------------------------------------------------------------|
| `()`   | Zero bytes.                                                    |

---

## 3. Length-prefixed and compound types

### 3.1 Strings (`String` / `&str`)

```
String  ::= varint(byte_length) bytes(byte_length)
```

- `byte_length` is the number of UTF-8 bytes that follow (not the codepoint
  count).
- `bytes(byte_length)` MUST be valid UTF-8. Decoders MUST reject invalid
  UTF-8 with **`InvalidUtf8`**.
- The empty string is encoded as `varint(0)` — a single `0x00` byte — with
  no following payload.

### 3.2 Fixed-size arrays (`[T; N]`)

```
[T; N]  ::= T_encoding × N
```

- `N` is part of the type, not the wire shape. There is no length prefix.
- Elements are encoded in array index order (`[0]`, `[1]`, …, `[N-1]`).

### 3.3 Variable-length lists (`Vec<T>` / `&[T]`)

```
Vec<T>  ::= varint(element_count) T_encoding × element_count
```

- `element_count` is the number of `T` values that follow.
- Decoders MUST validate `element_count` against the configured allocation
  cap (see §6.1) before allocating per-element capacity, and MUST reject
  counts that exceed it with **`InvalidLength`**.

### 3.4 Tuples

```
(T0, T1, …, Tn)  ::= T0_encoding T1_encoding … Tn_encoding
```

- Fields are encoded in declaration order, concatenated.
- No length prefix, no framing bytes between fields.
- The unit tuple `()` is zero bytes (see §2.5).

### 3.5 Optionals (`Option<T>`)

```
Option<T>  ::= None_tag
             | Some_tag T_encoding

None_tag  ::= 0x00
Some_tag  ::= 0x01
```

- A tag byte outside `{0x00, 0x01}` is **invalid** and decoders MUST
  reject it as **`InvalidTag { kind: "Option", tag }`**.

### 3.6 Results (`Result<T, E>`)

```
Result<T, E>  ::= Ok_tag  T_encoding
                | Err_tag E_encoding

Ok_tag   ::= 0x00
Err_tag  ::= 0x01
```

- A tag byte outside `{0x00, 0x01}` is **invalid** and decoders MUST
  reject it as **`InvalidTag { kind: "Result", tag }`**.

### 3.7 Enums

```
Enum   ::= varint(variant_index) variant_encoding
```



- `variant_index` is an unsigned LEB128 varint that selects the variant
  by its source-code declaration order, starting at `0`. The varint
  representation is the same as for `u64`, but the value MUST fit in
  `u32` (no real-world Rust enum has 4 billion variants).
- `variant_encoding` is the encoding of the variant's fields in source
  declaration order, concatenated, with no inter-field framing. Unit
  variants contribute zero bytes after the tag.
- A `variant_index` that does not correspond to any declared variant of
  the target type is **invalid** and decoders MUST reject it as
  **`UnknownVariant { kind, index }`**, where `kind` is the type's name
  and `index` is the offending value.

The variant index space is **source-order dependent**. Adding a variant in
the middle of an enum declaration is a **wire-format-breaking change for
that enum**, even though it does not break the codec spec. Append new
variants to the end of the declaration to maintain compatibility.

This encoding lands in `v0.4.0` and is the format the
`#[derive(Serialize, Deserialize)]` macros emit for enums.

### 3.8 Versioned structs

```
VersionedStruct  ::= varint(version) varint(body_len) body
body             ::= live_field_encoding *  (concatenated, no padding)
```

- `version` is the type's schema version, an unsigned `u32` LEB128 varint.
  Schema versions start at `1`; `0` is reserved and decoders MUST reject
  it (rationale: distinguishes a versioned payload from an accidental
  all-zeros buffer).
- `body_len` is an unsigned `u64` LEB128 varint giving the byte length of
  the body that follows. Decoders MUST validate `body_len` against the
  configured allocation cap (§6.1) **before** reading the body, and MUST
  refuse `body_len > max_alloc`.
- `body` is the concatenated encodings of every field that is **live at
  this version**, in source declaration order.

A field is *live at version V* iff:

```
live(field, V)  ⟺  field.since ≤ V  AND  (field.deprecated is None OR V < field.deprecated)
```

Where `field.since` defaults to `1` (always present from the first
version) and `field.deprecated` defaults to `None` (never removed).

**Cross-version decode contract.** A decoder for type `T` at *known
version* `K` reading a payload encoded at *wire version* `W`:

| Relationship | Behaviour                                                                                                  |
|--------------|------------------------------------------------------------------------------------------------------------|
| `W = K`      | Read every field that is live at `W`. Equivalent to the writer's encode.                                   |
| `W < K`      | Read every field that is live at `W`. For each `T` field with `since > W` (added after `W`), construct via `Default::default()`. |
| `W > K`      | Read every field that is live at `W` from the decoder's perspective — i.e. every field of `T` that the decoder knows is live up to `K`. Trailing bytes inside `body` are silently discarded; the length prefix bounds the read so the next message is not consumed. |

The third row is the property that makes append-only schema evolution
work: a newer producer can ship a `T` payload to an older consumer without
the consumer needing to know what was added. As long as new fields are
**only appended** to the end of the struct (and `since = N` is set
correctly), older consumers read what they understand and ignore the
rest.

**Rules for evolving a versioned struct compatibly:**

1. The struct-level `version` MUST be incremented by `1` each time a new
   field is appended or a field is deprecated.
2. New fields MUST be **appended** to the end of the struct declaration
   and carry `#[pack_io(since = N)]` where `N` is the new struct version.
3. Removed fields MUST be marked `#[pack_io(deprecated = N)]` and remain
   in the struct declaration until the next MAJOR (`2.x`) — they MUST NOT
   be deleted in a minor version.
4. Reordering fields, removing fields outright, or changing a field's
   type are all wire-format-breaking changes for that struct and require
   a major bump.
5. Fields with `since > 1` MUST have a `Default` impl — decoders use
   `Default::default()` for fields missing from older payloads.

Non-versioned structs (those without `#[pack_io(version = N)]`) use the
plain field-concatenation encoding from `v0.4.0`; the choice between
versioned and non-versioned is per-type and cannot be changed without
breaking the wire format for that type.

Encoders at struct version `V` MUST NOT emit fields with `deprecated ≤ V`
(the field is gone from `V`'s wire shape). Decoders reading a payload
that was encoded at `W < deprecated` MUST decode the field's value
normally; payloads at `W ≥ deprecated` produce `Default::default()` for
the field.

This encoding lands in `v0.5.0` and is the format the
`#[derive(Serialize, Deserialize)]` macros emit for types carrying
`#[pack_io(version = N)]`. Companion runtime helper:
`pack_io::peek_version(&bytes) -> Result<u32>` reads only the leading
`version` varint, leaving the rest of the buffer untouched.

---

## 4. Maps and sets

Maps and sets share the same wire shape:

```
Map<K, V>  ::= varint(entry_count) entry × entry_count
entry      ::= K_encoding V_encoding

Set<T>     ::= varint(element_count) T_encoding × element_count
```

### 4.1 Canonical ordering (the determinism contract)

The element / entry sequence MUST be sorted **lexicographically by the
encoded key bytes** (for maps, the key; for sets, the element itself).

**Sort rule:**

1. For each entry / element, compute its encoding into a temporary buffer.
2. Order entries by lexicographic byte comparison of the encoded **key**
   portion (for maps) or the encoded element (for sets). Earlier-differing
   bytes win; a shorter byte sequence whose contents are a prefix of a
   longer one is the smaller value.
3. Emit `varint(count)` followed by the encoded entries in the sorted
   order.

Encoders MUST follow this rule. Decoders MAY accept inputs whose entries
are not in canonical order (lenient decode), but SHOULD reject them in
contexts that depend on byte-determinism (signature verification,
content-addressing).

**Why sort by encoded bytes?** It is the only ordering that gives
`HashMap<K, V>` and `BTreeMap<K, V>` (over the same logical data) the same
wire-format output, regardless of `K`'s `Ord` impl or the absence thereof.
Workflows that hash or sign the encoded payload survive any later switch
between concrete map types.

**Duplicate keys / elements:** the canonical encoding produced by the
sort step contains no duplicates (the source collection already
de-duplicates). On decode, if a duplicate appears in the input, the
decoder accepts the encoding (last-write-wins for maps, first-occurrence
for sets) and does not signal an error. Producers MUST NOT emit
duplicates; consumers in strict contexts SHOULD reject them.

### 4.2 Supported map / set types in the standard library

| Type                    | Sort by                    | Decode collects into            |
|-------------------------|----------------------------|---------------------------------|
| `BTreeMap<K, V>`        | encoded-key bytes          | `BTreeMap<K, V>` (`K: Ord`)     |
| `BTreeSet<T>`           | encoded-element bytes      | `BTreeSet<T>` (`T: Ord`)        |
| `HashMap<K, V, S>`      | encoded-key bytes          | `HashMap<K, V, S>` (`K: Hash + Eq`, `S: BuildHasher + Default`) |
| `HashSet<T, S>`         | encoded-element bytes      | `HashSet<T, S>` (`T: Hash + Eq`, `S: BuildHasher + Default`) |

`HashMap` and `HashSet` encode through the `std` feature (default-on); they
are unavailable in `no_std` builds.

---

## 5. End-of-message handling

The wire format itself does **not** carry an end-of-message marker. Whether
a payload contains a single value or several depends on the surrounding
framing:

- The Tier-1 [`decode`](API.md#decode) entry point requires the input slice
  to be fully consumed; trailing bytes are rejected with
  **`TrailingBytes`**. Use this for single-value payloads.
- The Tier-2 [`Decoder`](API.md#decoder) / [`IoDecoder`](API.md#iodecoder)
  do not enforce full consumption — repeated `read()` calls extract
  successive values from the same buffer. Use this for multi-value
  streams or for length-framed protocols where the framing is handled
  externally.

---

## 6. Error categories

Decoders surface failures via the `SerialError` enum. The taxonomy below is
normative — implementers SHOULD map their own error types onto these
categories for interoperability.

| Category               | When raised                                                                                |
|------------------------|--------------------------------------------------------------------------------------------|
| `UnexpectedEof`        | Decoder needed more bytes than the input contained.                                        |
| `InvalidLength`        | A length prefix exceeded the buffer or the configured allocation cap (§6.1).               |
| `VarintOverflow`       | A LEB128 varint exceeded its target width.                                                 |
| `IntegerOutOfRange`    | A decoded integer did not fit in the requested narrower target.                            |
| `InvalidBool`          | A boolean byte was neither `0x00` nor `0x01`.                                              |
| `InvalidUtf8`          | A length-prefixed byte run was not valid UTF-8 when decoding a `String`.                   |
| `InvalidTag`           | An `Option` / `Result` tag byte was outside `{0x00, 0x01}`.                                |
| `TrailingBytes`        | A strict decode call left bytes unread.                                                    |
| `UnknownVariant`       | An enum variant index did not correspond to any declared variant of the target type (§3.7).|
| `Io` *(std-only)*      | The underlying `Read` / `Write` returned an `std::io::Error`.                              |

### 6.1 Allocation cap

Decoders MUST enforce a per-value maximum allocation size (`max_alloc`).
For every length-prefixed value (`String`, `Vec<u8>`, `Vec<T>` and the
element count of a collection), the decoder checks that the declared length
is less than or equal to `max_alloc` before performing any allocation.

A length that exceeds `max_alloc` is rejected with `InvalidLength` —
**before** the decoder allocates a single byte. This is the primary
defence against a hostile `length = u64::MAX` payload.

The default value of `max_alloc` is implementation-defined; pack-io ships
`1 GiB` as the default and exposes
[`Config::with_max_alloc`](API.md#config) for tighter caps.

---

## 7. Versioning of this document

| Version | Changes                                                                                        |
|---------|------------------------------------------------------------------------------------------------|
| `1.0`   | Initial freeze, shipped with pack-io `v0.3.0`. All sections above apply.                       |
| `1.1`   | Additive: enums (§3.7) and `UnknownVariant` error (§6). Shipped with pack-io `v0.4.0`. No existing encoding changes — payloads valid under `1.0` remain valid under `1.1`. |
| `1.2`   | Additive: versioned structs (§3.8). Shipped with pack-io `v0.5.0`. Plain (non-versioned) structs encode exactly as before; versioned structs are a new per-type opt-in via `#[pack_io(version = N)]`. |

Future revisions to this document will be additive — new types, new
optional headers, or new error categories — and MUST preserve the contracts
above. A revision that breaks any of §2 — §4 is a wire-format-breaking
change and requires a major version bump (`2.x`).

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
