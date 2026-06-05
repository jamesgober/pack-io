<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>pack-io</b><br>
    <sub><sup>v1.0 PERFORMANCE BASELINE</sup></sub>
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
        <a href="./PERFORMANCE.md" title="Performance overview"><b>PERFORMANCE</b></a>
        <span>&nbsp;│&nbsp;</span>
        <span>BASELINE</span>
    </sup>
</div>
<br>

> **Frozen baseline for the `1.x` line.** High-fidelity Criterion medians,
> 100 samples per benchmark, 10 s measurement window per sample, 2 s
> warmup. Downstream CI and regression checks diff against these numbers;
> any change exceeding a 5 % regression on any row here is treated as a
> defect per [`REPS.md`](../REPS.md).
>
> [`docs/PERFORMANCE.md`](./PERFORMANCE.md) carries the full per-row
> analysis, methodology notes, and discussion of intentional losses; this
> file is the frozen data table.

## Environment

| Property                | Value                                                                 |
|-------------------------|-----------------------------------------------------------------------|
| OS                      | Windows 11 Pro 10.0.26200 (x86_64)                                    |
| Toolchain               | Rust stable 1.95.x                                                    |
| Build profile           | `release` (`opt-level = 3, lto = "fat", codegen-units = 1, panic = "abort", strip = "symbols"`) |
| Criterion samples       | 100                                                                   |
| Measurement time        | 10.000 s per sample                                                   |
| Warmup time             | 2.000 s                                                               |
| pack-io version         | 1.0.0 (codec hot paths unchanged since v0.6.0)                        |
| bincode                 | 2.0.1, `config::standard()` (varint)                                  |
| postcard                | 1.x, `to_allocvec` / `from_bytes`                                     |
| rkyv                    | 0.8.16, `to_bytes` + `access::<ArchivedT>` + `deserialize`            |

Reproduce locally:

```bash
cargo bench --bench comparative --features derive -- \
    --warm-up-time 2 --measurement-time 10
```

## Baseline numbers

### Owned-decode of a borrow-heavy log record

Payload: `{ timestamp: u64, level: u8, message: String, tags: Vec<String>, payload: Vec<u8 × 256> }`.

| Codec | Encode | Decode (owned) | Decode (zero-copy) |
|---|---:|---:|---:|
| **pack-io** | **37.9 ns** | 158.9 ns | 34.7 ns |
| bincode | 39.4 ns | **165.1 ns** | — |
| postcard | 232.6 ns | 285.1 ns | — |
| rkyv | 112.6 ns | 154.6 ns | **12.0 ns** |

### Primitive round-trips

| Workload | pack-io | bincode | postcard |
|---|---:|---:|---:|
| `u64` encode + decode | 22.3 ns | **20.6 ns** | — |
| 64-byte `String` (owning) | **44.7 ns** | 48.8 ns | — |
| 64-byte `&str` (view) | **5.2 ns** | n/a | n/a |
| 4 KiB `Vec<u8>` decode | **59.7 ns** | 63.0 ns | — |

## Summary

- **Wins** *(pack-io fastest of the four)*: encode struct, owning
  `String` round-trip, zero-copy `&str` view, `Vec<u8>` 4 KiB decode.
- **Tied or near-tied** *(within 8 %)*: `u64` round-trip (bincode 1.08×
  faster), owned struct decode (rkyv 1.03× faster, pack-io ahead of
  bincode).
- **Intentional loss**: `decode_view` vs rkyv archived (~3× — rkyv reads
  a raw memory layout; pack-io walks varints by the wire-format spec).

The v0.6 release notes claimed the same picture from a 3 s measurement
window. The 10 s window confirms it with tighter confidence intervals;
no per-row median moves more than ~3 % vs the v0.6 quick-run numbers.

## Regression policy

Per [`REPS.md`](../REPS.md) §Performance:

- A change touching any codec hot path MUST run this benchmark and
  record results.
- A regression exceeding **5 %** on any row above blocks the merge.
- A win exceeding **5 %** on any row updates this file and the CHANGELOG.

The CI matrix does **not** run this benchmark — measurement variance
across CI runners is too high to be useful as a regression gate.
Performance regression checks happen on dedicated hardware between
releases.

## Why these workloads

The seven rows above are the workloads every real consumer hits:

- **Encode + decode of a borrow-heavy log record** is the shape of any
  message a service sends — id + name + a short payload + a small
  metadata vector. The encode path stresses every primitive impl in
  sequence; the decode path stresses allocation paths and the borrow
  vs owning trade-off.
- **`u64` round-trip** is the smallest primitive that exercises varint
  encoding, the most common length-prefix shape.
- **64-byte `String`** is the size most network strings (paths, IDs,
  short messages) land at.
- **4 KiB `Vec<u8>`** is the size most network buffers land at after
  one MTU's worth of framing.

A consumer running pack-io against a different workload should
benchmark that workload directly; these numbers are representative, not
exhaustive. The benchmark source in
[`benches/comparative.rs`](../benches/comparative.rs) is straightforward
to extend.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
