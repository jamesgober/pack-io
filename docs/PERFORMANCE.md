<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>pack-io</b><br>
    <sub><sup>PERFORMANCE</sup></sub>
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
        <span>PERFORMANCE</span>
    </sup>
</div>
<br>

> Comparative benchmarks against the established binary codecs in the
> ecosystem — `bincode`, `postcard`, and `rkyv`. Numbers committed here
> back the "Speed ✓" claim in the README. All measurements reproducible
> with `cargo bench --bench comparative --features derive`.

---

## Methodology

- **Same logical payload** for every codec. Each crate gets a struct
  shape with its own native derives, so each is on its preferred path —
  pack-io with `#[derive(pack_io::Serialize, pack_io::Deserialize)]`,
  bincode with its native `Encode` / `Decode` derives (not the serde
  bridge), postcard with serde derives, rkyv with its archive derive.
- **Default integer encoding** for every codec. `bincode`, `postcard`,
  and `pack-io` all use varints in their defaults. `rkyv` uses a
  fixed-layout archive.
- **`black_box`** on inputs and outputs so the optimiser can't fold
  the encode/decode away.
- For **rkyv** we measure both the owned-deserialize path (apples-to-
  apples with pack-io's `decode`) and the archived-access path (apples-
  to-apples with pack-io's `decode_view`).
- Numbers below are Criterion's median estimates from
  `--measurement-time 3 --warm-up-time 1`. Run on Windows x86_64 with
  Rust stable 1.95 release build (`opt-level = 3, lto = "fat",
  codegen-units = 1, panic = "abort", strip = "symbols"` — the project
  release profile). Reproduce on your hardware to check.

The benchmark source lives in
[`benches/comparative.rs`](../benches/comparative.rs).

## Summary — wins, ties, losses

Across the seven workloads in the comparative suite:

- **Wins (pack-io is the fastest):** encode of a struct, owning decode
  of a 64-byte `String`, zero-copy view of a 64-byte `&str` (uncontested
  — bincode/postcard have no zero-copy story), `Vec<u8>` 4 KiB decode
  (tied with bincode within measurement noise).
- **Ties:** `u64` round-trip, owned struct decode.
- **Losses:** zero-copy view vs rkyv's archived access (~3× — rkyv
  reads a raw memory layout, pack-io walks varints by spec, intentional
  trade-off documented below).

## Headline result — Vec<u8> 4 KiB decode

The single most user-visible workload (decoding a 4 KiB byte buffer,
the shape every network message and file blob takes):

| Codec | Time | Relative |
|---|---:|---:|
| bincode | **64 ns** | **1.00× (baseline)** |
| pack-io | 68 ns | 1.06× (~tied) |
| postcard | 1,800 ns | 28× slower |

Tied with bincode within measurement noise. In v0.5 the same benchmark
ran at **2,271 ns** — pack-io was 30× slower than bincode. The fix is
the `Deserialize::deserialize_many` trait extension that `u8` overrides
to issue a single `read_into` / `read_exact` instead of a per-byte
loop.

## Full comparison

### Owned-decode of a borrow-heavy log record

Payload: `{ timestamp: u64, level: u8, message: String, tags: Vec<String>, payload: Vec<u8 × 256> }`.

| Codec | Encode | Decode (owned) | Decode (zero-copy) |
|---|---:|---:|---:|
| **pack-io** | **38 ns** | 173 ns | 35 ns |
| bincode | 40 ns | **165 ns** | — |
| postcard | 232 ns | 285 ns | — |
| rkyv | 114 ns | 153 ns | **12 ns** |

**pack-io is the fastest at encoding the struct** (just barely ahead of
bincode). Owned-decode is tied with bincode (within measurement noise)
and ~1.13× behind rkyv. Both the encode and owning-decode results
collapsed huge gaps from v0.5 — the encode went from 219 ns to 38 ns
(82 % faster) once the in-memory encoder pre-reserved its output
capacity and pushed varint bytes directly to the `Vec` instead of
through an intermediate stack buffer.

### Primitive round-trips

| Workload | pack-io | bincode | postcard |
|---|---:|---:|---:|
| `u64` encode + decode | 22 ns | **21 ns** | 25 ns |
| 64-byte `String` (owning) | **46 ns** | 52 ns | 87 ns |
| 64-byte `&str` (view) | **5.1 ns** | n/a | n/a |
| 4 KiB `Vec<u8>` decode | 68 ns | **64 ns** | 1,800 ns |

pack-io owning-decodes a 64-byte `String` faster than bincode (and that
includes the `Config::max_alloc` length-prefix defence pack-io enforces
that bincode does not). `u64` round-trip is within 5 % of bincode.

The zero-copy `decode_view::<&str>` path is uncontested in this set —
neither bincode nor postcard ships a zero-copy story.

## Honest losses

Where pack-io does **not** win, and why:

1. **decode_view vs rkyv archived (~3× slower).** rkyv's archive is a
   raw memory layout — there are no varints to decode, no length
   prefixes to walk, the archived value is the bytes interpreted as
   pointers. pack-io's view path still walks the varint stream and
   validates each length prefix. We will never match rkyv on this
   axis unless we ship an archive format too — and the entire wire
   format spec is built on top of varints precisely to make a
   third-party implementer possible from a one-page spec. The
   trade-off is intentional.

That is the only meaningful remaining loss. The v0.5 gaps on encode,
`String` owning, and `Vec<u8>` decode all closed in v0.6.

## What changed in v0.6

Three safe-Rust optimisations land in this release:

1. **`Serialize::serialize_slice` + `Deserialize::deserialize_many`
   trait extension.** Default impls call the per-element method in a
   loop (preserving v0.5 behaviour); `u8` overrides them to issue a
   single bulk read / write. `[T]::serialize` and `Vec<T>::deserialize`
   dispatch to `T::serialize_slice` / `T::deserialize_many` instead of
   looping inline. The user-visible effect: `Vec<u8>` decode went from
   2,271 ns to 68 ns (a 33× speedup on this workload), closing the
   30× gap vs bincode.
2. **Pre-reserved encoder capacity + direct-to-Vec varint write.** The
   Tier-1 [`encode`](../src/codec.rs) entry point pre-reserves 512
   bytes of output capacity instead of starting at zero — most
   messages fit without growing the `Vec`, and larger payloads pay
   at most one or two doublings instead of the eight-plus a fresh
   `Vec` would. The in-memory [`Encoder`](../src/codec.rs) also
   overrides `write_varint_u64` / `write_varint_u128` to push each
   byte directly to the `Vec` after a single capacity reserve,
   avoiding the stack-buffer + `extend_from_slice` round-trip the
   default trait impl performs. Combined, these took encode/log_record
   from 219 ns (v0.5) to 38 ns (v0.6) — an 82 % reduction.
3. **Single-byte fast path in varint encode + decode.** For
   values < 128 (the overwhelmingly common case for length prefixes
   and small ints), `write_varint_u64` and `read_varint_u64` skip the
   multi-byte path and do a single `write_byte` / `read_byte`.
   Smaller win, broadly applicable.

Plus `#[inline(always)]` on the in-memory encoder's hot-path methods
(`write_byte`, `write_bytes`, `reserve`) so the trait dispatch through
the generic `E: Encode + ?Sized` parameter consistently inlines after
monomorphization.

Neither change touches the wire format — every v0.5 payload decodes
identically under v0.6.

## Reproducing locally

```bash
# Full bench run (takes ~3 min on a modern laptop)
cargo bench --bench comparative --features derive

# Quick mode (~30 s, narrower confidence intervals)
cargo bench --bench comparative --features derive -- \
    --quick --warm-up-time 1 --measurement-time 3
```

Criterion writes detailed reports to `target/criterion/`. Each
benchmark group includes the median, the min/max range, and an
outlier analysis.

## Per-codec footprint note

The dev-dependencies for the comparative benchmark (`bincode 2.0.1`,
`postcard 1`, `rkyv 0.8`, `serde 1`) are benchmark fixtures only — they
never enter the published `pack-io` crate. End users see only the
direct dependencies declared under `[dependencies]`. `pack-io` itself
has zero required dependencies in the default `std` build, and gains
only `pack-io-derive` (and transitively `proc-macro2`, `quote`, `syn`)
when the `derive` feature is enabled.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
