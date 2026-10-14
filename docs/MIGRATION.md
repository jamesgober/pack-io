<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>pack-io</b><br>
    <sub><sup>MIGRATION — 1.x TO 2.0</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="../CHANGELOG.md" title="Changelog"><b>CHANGELOG</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./API.md" title="API Reference"><b>API</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./WIRE_FORMAT.md" title="Wire Format Spec"><b>WIRE FORMAT</b></a>
        <span>&nbsp;│&nbsp;</span>
        <span>MIGRATION</span>
    </sup>
</div>
<br>

> **The bytes do not change.** pack-io 2.0 reads every payload a 1.x
> encoder wrote for valid data and writes the same bytes 1.x wrote for
> the same value (pinned by golden vectors written by 1.0.1). You can
> upgrade readers and writers in any order. What changed is how strictly
> decoders treat hostile input, what the derive accepts, and one new
> blanket impl.

---

## Checklist

1. Bump the dependency: `pack-io = "2"` (`pack-io-derive` follows; it is
   pinned exactly).
2. Build. Compile errors can come from only three places:
   - a `#[pack_io(...)]` attribute 1.x ignored or accepted wrongly, or
     an enum discriminant that differs from the variant's wire tag
     ([§4](#4-derive-attributes-1x-ignored-are-errors));
   - `deprecated = N` ([§6](#6-deprecated--n-is-now-removed--n-or-retired--n));
   - your own `impl … for Box<YourType>` ([§7](#7-boxt-has-blanket-impls)).
3. Search for `#[pack_io(` inside `enum` declarations
   ([§5](#5-attributes-on-enums-are-honoured)). This is the only change
   that compiles silently and changes bytes.
4. If you decode deeply nested trusted data or huge streams, raise the
   limits ([§1](#1-nesting-depth-is-limited), [§3](#3-decode_from-is-bounded-by-max_alloc),
   [§9](#9-elements-that-occupy-no-input-bytes-share-one-budget)).
5. If you match on errors, see [§2](#2-version-0-is-rejected) and
   [§8](#8-a-zero-max_alloc-is-invalidconfig).

Everything below shows 1.x code first and 2.0 code second.

---

## 1. Nesting depth is limited

Every derived decoder now counts one level against `Config::max_depth`
(default 128). Deeper input fails with
`SerialError::DepthLimitExceeded` instead of overflowing the stack,
which 1.x did on about 5 KB of hostile input.

```rust,ignore
// 1.x: no limit (and a stack overflow on hostile input).
let tree: Tree = pack_io::decode(&bytes)?;
```

```rust
use pack_io::{Config, Deserialize, Serialize, decode_with_config, encode};

#[derive(Serialize, Deserialize)]
struct Tree { children: Vec<Tree> }

let mut tree = Tree { children: Vec::new() };
for _ in 0..200 {
    tree = Tree { children: vec![tree] };
}
let bytes = encode(&tree).unwrap();

// 2.0: trusted data deeper than 128 levels needs a higher limit.
let deep = Config::new().with_max_depth(1024);
let back: Tree = decode_with_config(&bytes, deep).unwrap();
# let _ = back;
```

Mind the stack when you raise it: each level costs one call frame of the
type's decoder (about 3 KiB for a five-field struct in a debug build on
Windows x86_64).

## 2. Version 0 is rejected

A versioned struct whose payload says version `0` now fails with
`SerialError::InvalidVersion { version: 0 }`. The spec has required this
since 0.5, and no pack-io encoder writes version 0, so only hand-made or
corrupted payloads are affected.

```rust
use pack_io::{Deserialize, SerialError, decode};

#[derive(Deserialize, Debug)]
#[pack_io(version = 1)]
struct Account { id: u64 }

// varint(version = 0) ++ varint(body_len = 1) ++ body
let err = decode::<Account>(&[0, 1, 7]).unwrap_err();
assert!(matches!(err, SerialError::InvalidVersion { version: 0 }));
```

## 3. `decode_from` is bounded by `max_alloc`

`decode_from` used to read the whole stream into memory. It now stops
after `max_alloc` bytes (default 1 GiB) and fails with `InvalidLength`.

```rust,ignore
// 1.x: reads everything, however large.
let value: Snapshot = pack_io::decode_from(&mut file)?;
```

```rust
use std::io::Cursor;
use pack_io::{Config, decode_from_with_config};

let mut reader = Cursor::new(pack_io::encode(&vec![0u8; 16]).unwrap());
// 2.0: allow up to 4 GiB for this trusted source.
let big = Config::new().with_max_alloc(4 << 30);
let value: Vec<u8> = decode_from_with_config(&mut reader, big).unwrap();
# assert_eq!(value.len(), 16);
```

## 4. Derive attributes 1.x ignored are errors

Unknown or duplicated attributes, attributes in the wrong place, and
`since` newer than the struct's `version` are compile errors pointing at
the attribute. In 1.x they did nothing. A too-new `since` was worse: it
silently dropped the field from the encoding.

```rust,ignore
// 1.x: compiles; `email` is never encoded (since > version).
#[derive(pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 2)]
struct User {
    id: u64,
    #[pack_io(since = 3)]
    email: String,
}
```

```rust
// 2.0: give the struct the version that introduces the field.
#[derive(pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 3)]
struct User {
    id: u64,
    #[pack_io(since = 3)]
    email: String,
}
```

For an unknown attribute, delete it or use the supported attribute the
error lists.

Two more mistakes are now compile errors:

- **`removed = N` more than one version ahead of `version`.** A reader
  may announce a removal for the next version (the reader-first rollout
  in [§6](#6-deprecated--n-is-now-removed--n-or-retired--n)); a 1.x
  `deprecated = 40` on a version-2 struct is almost certainly a typo.
  (`retired = N` must not exceed `version` at all: above it, it does
  nothing.)
- **A Rust discriminant that differs from the wire tag.** pack-io has
  always encoded a variant by its position, never by `= N`. 1.x ignored
  the number silently; 2.0 asks you to say which you mean.

```rust,ignore
// 1.x: compiles; `Warn` is encoded as tag 1, not 7.
#[derive(pack_io::Serialize, pack_io::Deserialize)]
enum Level {
    Info = 0,
    Warn = 7,
}
```

```rust
// 2.0, same bytes as 1.x: pin the tag the variant always had. A
// discriminant equal to its tag (`Info = 0`) needs nothing.
#[derive(pack_io::Serialize, pack_io::Deserialize)]
enum Level {
    Info = 0,
    #[pack_io(index = 1)]
    Warn = 7,
}
# assert_eq!(pack_io::encode(&Level::Warn).unwrap(), [1]);
# let _ = Level::Info;
```

Use `#[pack_io(index = 7)]` instead to encode the discriminant (that
changes the bytes).

## 5. Attributes on enums are honoured

1.x ignored every `#[pack_io(...)]` attribute on enum variants and enum
fields. 2.0 applies `skip`, `default`, `with` and `index` there, so an
enum that carried one of those attributes **encodes differently**. This
is the one change that compiles without a warning.

```rust,ignore
// 1.x: `cache` was encoded anyway; the attribute had no effect.
enum Msg {
    Data { id: u32, #[pack_io(skip)] cache: Vec<u8> },
}
```

```rust
// 2.0: to keep the 1.x bytes, remove the attribute.
#[derive(pack_io::Serialize, pack_io::Deserialize)]
enum Msg {
    Data { id: u32, cache: Vec<u8> },
}

let bytes = pack_io::encode(&Msg::Data { id: 1, cache: vec![9] }).unwrap();
assert_eq!(bytes, [0, 1, 1, 9]); // variant, id, cache length, cache byte
```

If you do want the 2.0 behaviour (for example `index` to pin variant
tags), keep the attribute but treat it as a format change: roll it out
like any other change to a type's encoding.

## 6. `deprecated = N` is now `removed = N` or `retired = N`

`#[pack_io(deprecated = N)]` is a compile error in 2.0, and the error
names the two replacements:

- **`removed = N`** is exactly the 1.x encoding. The field's bytes
  leave the body from version `N`. Use it for every field that was
  already deprecated in 1.x, so payloads you have already written at
  version `N` keep decoding. It still needs the reader-first rollout:
  a reader that predates the removal decodes the next field's bytes
  into the removed field.
- **`retired = N`** is new in 2.0. From version `N` the field's slot
  stays on the wire, holding its default value, and readers at version
  `N` or later ignore it. Every reader stays aligned, including
  readers built before the change, which see the default. It costs
  about one byte per value. Use it for **new** retirements.

```rust,ignore
// 1.x
#[derive(pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 3)]
struct Account {
    id: u64,
    #[pack_io(deprecated = 3)]
    legacy_token: String,
    #[pack_io(since = 2)]
    handle: String,
}
```

```rust
// 2.0, already-deployed deprecation: same bytes as 1.x.
#[derive(pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 3)]
struct Account {
    id: u64,
    #[pack_io(removed = 3)]
    legacy_token: String,
    #[pack_io(since = 2)]
    handle: String,
}
```

```rust
// 2.0, retiring a field in a new version (4): safe with old readers.
#[derive(pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 4)]
struct Account {
    id: u64,
    #[pack_io(retired = 4)]
    legacy_token: String,
    #[pack_io(since = 2)]
    handle: String,
}
```

Do **not** turn an existing `deprecated = N` into `retired = N` for the
same `N`. Version-`N` payloads written by 1.x have no slot for the field,
and a `retired = N` reader expects one. Retire fields only at a new
version.

## 7. `Box<T>` has blanket impls

`Box<T>` now implements `Serialize`, `Deserialize` and `DeserializeView`
whenever `T` does, and encodes exactly like `T`. `Box` is
`#[fundamental]`, so an impl you wrote for `Box<YourType>` now conflicts
with the blanket impl (E0119). Delete it; the bytes are the same.

```rust,ignore
// 1.x: needed for recursive types.
impl pack_io::Serialize for Box<Node> { /* forwards to Node */ }
impl pack_io::Deserialize for Box<Node> { /* forwards to Node */ }
```

```rust
// 2.0: nothing to write.
#[derive(pack_io::Serialize, pack_io::Deserialize, PartialEq, Debug)]
struct Node {
    value: u8,
    next: Option<Box<Node>>,
}

let list = Node { value: 1, next: Some(Box::new(Node { value: 2, next: None })) };
let bytes = pack_io::encode(&list).unwrap();
assert_eq!(bytes, [1, 1, 2, 0]);
assert_eq!(pack_io::decode::<Node>(&bytes).unwrap(), list);
```

`#[pack_io(with = "...")]` modules written for `Box` fields keep
working; you can drop them.

## 8. A zero `max_alloc` is `InvalidConfig`

`Config::with_max_alloc(0)` is rejected when the decoder is built, as in
1.x, but the error is now `InvalidConfig` (matching `max_depth == 0`),
not `InvalidLength { declared: 0, .. }`.

```rust,ignore
// 1.x
assert!(matches!(err, SerialError::InvalidLength { declared: 0, .. }));
```

```rust
use pack_io::{Config, Decoder, SerialError};

let err = Decoder::with_config(&[], Config::new().with_max_alloc(0)).unwrap_err();
assert!(matches!(err, SerialError::InvalidConfig { .. }));
```

## 9. Elements that occupy no input bytes share one budget

A collection element whose encoding is empty (`Box<()>`, `Rc<()>`, a
zero-sized type, a struct whose fields are all `skip`) is not backed by
any input, so a three-byte count could demand gigabytes. 2.0 charges
each such element `size_of` the element (at least one byte) against
`max_alloc`, summed over the whole decode, and fails with
`InvalidLength` beyond it. Ordinary elements, which consume at least one
input byte each, are not affected.

```rust,ignore
// 1.x: 4 bytes declaring 2^27 `Box<()>` allocated 1 GiB.
let v: Vec<Box<()>> = pack_io::decode(&[0x80, 0x80, 0x80, 0x40]).unwrap();
```

```rust
use pack_io::{Config, SerialError, decode_with_config};

// 2.0: under a 1 MiB budget, 2^20 `Box<()>` (8 MiB) is refused...
let count = [0x80, 0x80, 0x40]; // varint(2^20)
let tight = Config::new().with_max_alloc(1 << 20);
let err = decode_with_config::<Vec<Box<()>>>(&count, tight).unwrap_err();
assert!(matches!(err, SerialError::InvalidLength { .. }));
// ...and legitimate data well inside the budget decodes as before.
let ok: Vec<Box<()>> = pack_io::decode(&pack_io::encode(&vec![Box::new(()); 1000]).unwrap()).unwrap();
assert_eq!(ok.len(), 1000);
```

If you legitimately decode more than `max_alloc` bytes' worth of such
elements in one message, raise `max_alloc`.

---

## Changes that need no code

- **Bounded allocation.** Hostile length prefixes now fail before
  allocating. Valid input decodes as before.
- **Versioned views.** `#[derive(DeserializeView)]` on a
  `#[pack_io(version = N)]` struct now reads the versioned frame. 1.x
  mis-parsed it, so any 1.x code that "worked" with it was reading
  garbage.
- **New `SerialError` variants**: `DepthLimitExceeded`,
  `NonCanonical`, `InvalidVersion`, `InvalidValue`, `InvalidConfig` and
  `Custom`. The enum is `#[non_exhaustive]`, so your `match` already has
  a wildcard arm. Existing variants keep their discriminants.
- **New trait methods** on `Decode`, `Deserialize`, `DeserializeView`
  and `Serialize` all have default implementations. Custom decoders and
  hand-written impls compile unchanged.
- **Canonical decoding** is new and opt-in:
  `Config::with_canonical(true)`. Turn it on where encoded bytes are
  hashed or signed. It works on the in-memory decoder;
  `IoDecoder::with_config` rejects it, so buffer the frame with
  `decode_from_with_config` instead. A canonical decoder reads a
  versioned struct only at its own version (older and newer frames are
  `NonCanonical`); decode those leniently.
- **serde sets.** If you adopt the new serde adapter for types that hold
  a `BTreeSet` or `HashSet`, mark those fields
  `#[serde(with = "pack_io::serde::sorted_set")]` to get the native set
  bytes. Without it a set is written in iteration order (a `HashSet`
  differently in every process).

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
