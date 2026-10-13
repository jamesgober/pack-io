<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>pack-io</b><br>
    <sub><sup>v1.1 PERFORMANCE BASELINE</sup></sub>
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

> **Reference baseline for the `1.x` line, re-measured for v1.1.0.**
> Regression checks diff against these numbers; a change exceeding a 5 %
> regression on any row is treated as a defect per [`REPS.md`](../REPS.md).
> [`docs/PERFORMANCE.md`](./PERFORMANCE.md) carries the analysis; this
> file is the data.

## Environment and method

| Property | Value |
|---|---|
| CPU | AMD Ryzen 9 9950X3D (16 cores / 32 threads) |
| OS | Ubuntu under WSL2 (kernel 6.6.87.2-microsoft-standard-WSL2) on a Windows 11 Pro 10.0.26200 host |
| Toolchain | rustc 1.99.0; bench profile `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` |
| Harness | [`benches/min_trials.rs`](../benches/min_trials.rs): fastest of 300 trials × 2,000 iterations, process pinned to one core (`taskset -c 7`), best of 3 runs |
| Codec versions | pack-io 1.1.0, bincode 2.0.1 (`config::standard()`, varint), postcard 1.1.3, rkyv 0.8.18 |
| Measured | 2026-10-13 |

**Why not Criterion's mean this time.** The 1.0 table was taken with
Criterion (100 samples × 10 s) on an idle machine. The machine used for
1.1 was shared with other heavy workloads, and Criterion's estimates moved
by up to ±40 % between runs for every codec at once — bincode, whose code
did not change, swung as much as pack-io. The minimum over many short
trials on a pinned core is far less sensitive to that interference, so it
is the statistic published here. The Criterion suites
([`benches/comparative.rs`](../benches/comparative.rs),
[`benches/hot_paths.rs`](../benches/hot_paths.rs),
[`benches/codec_bench.rs`](../benches/codec_bench.rs)) remain the tool for
runs on a quiet machine. Because the statistic and the OS changed, these
numbers are **not** directly comparable with the 1.0 table (minimums are
lower than means); the 1.0.1 → 1.1.0 comparison below was measured with
the same harness on both versions.

Reproduce:

```bash
cargo bench --bench min_trials --features derive
```

## Comparative numbers (ns per operation, lower is better)

Payload for the struct rows: `{ timestamp: u64, level: u8, message: String,
tags: Vec<String> (4), payload: Vec<u8> (256) }`.

| Workload | pack-io | bincode | postcard | rkyv |
|---|---:|---:|---:|---:|
| struct encode | 24.4 | **24.0** | 134.0 | 53.6 |
| struct decode (owned) | 57.2 | 153.2 | 134.3 | **55.1** |
| struct decode (zero-copy view / archived access) | 25.4 | — | — | **12.8** |
| `u64` encode + decode | **5.7** | 7.7 | 6.6 | — |
| 64-byte `String` encode + decode | **16.6** | 19.8 | 32.4 | — |
| 64-byte `&str` view decode | **5.4** | — | — | — |
| 4 KiB `Vec<u8>` decode | **31.3** | 48.8 | 1,137 | — |

## 1.0.1 → 1.1.0 (same harness, same machine)

Both versions built from source with the same benchmark code and run in
alternating sessions; best of all sessions per side. Differences under
about 5 % are within run-to-run noise on this machine.

| Workload | 1.0.1 | 1.1.0 | Change |
|---|---:|---:|---:|
| versioned struct encode | 35.0 | 14.1 | −60 % |
| versioned struct decode | 22.3 | 13.2 | −41 % |
| `BTreeMap<String, u64>` × 100 encode | 2,891 | 1,070 | −63 % |
| `BTreeMap<u32, u32>` × 100 encode | 3,758 | 3,088 | −18 % |
| `HashMap<u64, u64>` × 100 encode | 3,804 | 3,107 | −18 % |
| `[u8; 32]` encode | 13.4 | 5.5 | −59 % |
| `[u8; 32]` decode | 14.0 | 0.8 | −94 % |
| 64 KiB `Vec<u8>` decode | 739 | 496 | −33 % |
| log record decode (owned) | 67.3 | 58.7 | −13 % |
| log record encode | 23.3 | 22.7 | −3 % |
| log record view | 24.5 | 24.2 | −1 % |
| plain struct encode | 12.5 | 12.5 | 0 % |
| plain struct decode | 11.1 | 11.0 | −1 % |
| `BTreeMap<u32, u32>` × 100 decode | 809 | 805 | 0 % |
| `Vec<u64>` × 1000 decode | 978 | 999 | +2 % |
| `IoDecoder` 4 KiB `String` | 111 | 115 | +3 % |
| `u64` round-trip | 5.3 | 5.4 | +1 % |
| 64-byte `String` round-trip | 15.3 | 15.2 | −1 % |
| 64-byte `&str` view round-trip | 12.2 | 12.3 | +1 % |

The same comparison on the Windows host (MSVC, rustc 1.95, unpinned)
agrees: every unchanged row within ±3 %, the changed rows faster.

## Summary

- **pack-io is the fastest of the four** on the `u64` and 64-byte
  `String` round-trips, the `&str` view (uncontested), and 4 KiB
  `Vec<u8>` decode.
- **Tied:** struct encode with bincode (within 2 %); owned struct decode
  with rkyv (within 4 %), both well ahead of bincode and postcard on this
  payload.
- **Intentional loss:** the zero-copy struct view is about 2× slower than
  rkyv's archived access — rkyv reads a fixed memory layout, pack-io
  walks varints as the wire spec defines.
- 1.1's hardening costs nothing measurable on the unchanged paths and
  makes several paths substantially faster.

## Regression policy

Per [`REPS.md`](../REPS.md) §Performance:

- A change touching any codec hot path MUST run these benchmarks and
  record results.
- A regression exceeding **5 %** on any row blocks the merge.
- A win exceeding **5 %** on any row updates this file and the CHANGELOG.

CI does not run these benchmarks; shared runners are too noisy for a 5 %
gate. Run them on dedicated hardware, before and after the change, with
the same harness.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
