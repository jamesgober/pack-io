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

## Headline result — Vec<u8> 4 KiB decode

The single most user-visible workload (decoding a 4 KiB byte buffer,
the shape every network message and file blob takes):

| Codec | Time | Relative |
|---|---:|---:|
| **pack-io** | **59 ns** | **1.00× (baseline)** |
| bincode | 76 ns | 1.29× slower |
| postcard | 1,774 ns | 30× slower |

pack-io reads the 4 KiB run in a single memcpy via the
[`Deserialize::deserialize_many`](../src/traits.rs) trait method that
`u8` overrides — every other `Vec<T>` decode still goes through the
generic per-element loop, but `Vec<u8>` short-circuits straight to
`Read::read_exact` (or the slice-borrow equivalent).

## Full comparison

### Owned-decode of a borrow-heavy log record

Payload: `{ timestamp: u64, level: u8, message: String, tags: Vec<String>, payload: Vec<u8 × 256> }`.

| Codec | Encode | Decode (owned) | Decode (zero-copy) |
|---|---:|---:|---:|
| pack-io | 136 ns | **161 ns** | **37 ns** |
| bincode | **39 ns** | 166 ns | — |
| postcard | 235 ns | 287 ns | — |
| rkyv | 115 ns | 154 ns | **12 ns** |

Owned-decode of the representative record is **tied with rkyv** and
**slightly faster than bincode**. The encode side runs ~3.5× behind
bincode because bincode's derive emits aggressively inlined per-field
code with no internal trait dispatch. Closing that gap is on the post-1.0
roadmap.

### Primitive round-trips

| Workload | pack-io | bincode | postcard |
|---|---:|---:|---:|
| `u64` encode + decode | 27 ns | **22 ns** | 26 ns |
| 64-byte `String` (owning) | 76 ns | **50 ns** | 87 ns |
| 64-byte `&str` (view) | **5.0 ns** | n/a | n/a |
| 4 KiB `Vec<u8>` decode | **59 ns** | 76 ns | 1,774 ns |

The zero-copy `decode_view::<&str>` path is uncontested in this set —
neither bincode nor postcard ships a zero-copy story.

For varint-heavy primitive paths (`u64`, small `String`), bincode is
~1.2–1.5× faster. The gap is bincode's tighter inlined codegen; the
work being done is roughly the same (varint encode of an integer,
length-prefixed UTF-8 of a string).

## Honest losses

Where pack-io does **not** win, and why:

1. **encode/log_record vs bincode (3.5× slower).** bincode's derive
   emits a single straight-line `write_u64; write_u8; write_string; …`
   sequence with no intermediate trait dispatch. pack-io's generated
   code calls `Serialize::serialize` per field, which monomorphises but
   still pays one stack-buffer round-trip per varint. This is the next
   target for the post-1.0 work — likely a custom code path emitted by
   the derive macro that writes directly to the encoder buffer without
   the per-field method call.
2. **u64 round-trip vs bincode (1.2× slower).** Same root cause.
   Acceptable for v0.6.
3. **string64 owning vs bincode (1.5× slower).** bincode reads the
   length + allocates + copies in one fused operation. pack-io reads
   the length, validates it against `max_alloc`, then does the copy.
   The validation step (a single integer compare against
   `Config::max_alloc`) is a defence-in-depth feature that bincode does
   not match — its decoder will happily try to allocate a u64::MAX-byte
   String if the length prefix says so. Worth the 25 ns for the safety.
4. **decode_view vs rkyv archived (3× slower).** rkyv's archive is a
   raw memory layout — there are no varints to decode, no length
   prefixes to walk, the archived value is the bytes interpreted as
   pointers. pack-io's view path still walks the varint stream and
   validates each length prefix. We will never match rkyv on this
   axis unless we ship an archive format too — and the entire wire
   format spec is built on top of varints precisely to make a
   third-party implementer possible from a one-page spec. The trade-off
   is intentional.

## What changed between v0.5 and v0.6

Two safe-Rust optimisations land in this release:

1. **`Serialize::serialize_slice` + `Deserialize::deserialize_many`
   trait extension.** Default impls call the per-element method in a
   loop (preserving v0.5 behaviour); `u8` overrides them to issue a
   single bulk read / write. `[T]::serialize` and `Vec<T>::deserialize`
   dispatch to `T::serialize_slice` / `T::deserialize_many` instead of
   looping inline. The user-visible effect: `Vec<u8>` decode goes from
   2,271 ns → 59 ns (a 38× speedup on this workload), and we now beat
   bincode on the byte-run hot path.
2. **Single-byte fast path in varint encode + decode.** For
   values < 128 (the overwhelmingly common case for length prefixes
   and small ints), `write_varint_u64` and `read_varint_u64` skip the
   stack-buffer round-trip and do a single `write_byte` / `read_byte`.
   Smaller win, broadly applicable.

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

## Roadmap

The post-1.0 performance work targets the gaps documented above:

- **encode codegen** — replace per-field trait dispatch with direct
  encoder-buffer writes emitted from the derive macro. Goal: close the
  3.5× encode gap vs bincode without breaking the wire format.
- **String decode** — fuse the length-validate + allocate + copy path.
  Goal: match bincode on string round-trips while keeping the
  `max_alloc` defence.
- **decode codegen** — same as encode, but on the decode side. The
  derive macro could emit a code path that pulls fields directly from
  the decoder rather than going through `T::deserialize` per field.

These are pure optimisations against the existing public surface — the
feature freeze at v0.5 holds.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
