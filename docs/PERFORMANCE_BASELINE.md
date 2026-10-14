<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>pack-io</b><br>
    <sub><sup>v2.0 PERFORMANCE BASELINE</sup></sub>
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

> **Reference baseline for the `2.x` line, measured for v2.0.0.**
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
| Harness | [`benches/min_trials.rs`](../benches/min_trials.rs): fastest of 300 trials × 2,000 iterations, process pinned to one core (`taskset -c 7`), best of 3 runs (6 alternating runs per side for the version comparison) |
| Codec versions | pack-io 2.0.0, bincode 2.0.1 (`config::standard()`, varint), postcard 1.1.3, rkyv 0.8.18 |
| Measured | 2026-10-13 |

**Why not Criterion's mean this time.** The 1.0 table was taken with
Criterion (100 samples × 10 s) on an idle machine. The machine used for
2.0 was shared with other heavy workloads, and Criterion's estimates moved
by up to ±40 % between runs for every codec at once — bincode, whose code
did not change, swung as much as pack-io. The minimum over many short
trials on a pinned core is far less sensitive to that interference, so it
is the statistic published here. The Criterion suites
([`benches/comparative.rs`](../benches/comparative.rs),
[`benches/hot_paths.rs`](../benches/hot_paths.rs),
[`benches/codec_bench.rs`](../benches/codec_bench.rs)) remain the tool for
runs on a quiet machine. Because the statistic and the OS changed, these
numbers are **not** directly comparable with the 1.0 table (minimums are
lower than means); the 1.0.1 → 2.0.0 comparison below was measured with
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
| struct encode | 27.2 | **26.2** | 144.9 | 59.3 |
| struct decode (owned) | 63.2 | 167.5 | 142.6 | **58.0** |
| struct decode (zero-copy view / archived access) | 25.1 | — | — | **13.6** |
| `u64` encode + decode | **5.8** | 8.2 | 6.7 | — |
| 64-byte `String` encode + decode | **17.1** | 21.0 | 33.0 | — |
| 64-byte `&str` view decode | **5.2** | — | — | — |
| 4 KiB `Vec<u8>` decode | **33.6** | 52.2 | 1,208 | — |

The machine ran about 5–10 % slower for every codec than in the 1.1
drafts' session (bincode's struct encode, whose code did not change, went
from 24.0 to 26.2 ns), so compare ratios between codecs, not these
absolute numbers with older tables.

## 1.0.1 → 2.0.0 (same harness, same machine)

Both versions built from source with the same benchmark code (1.0 API
only) and run in alternating sessions; best of all sessions per side.
Rows that moved by up to about ±6 % between sessions are marked *noise*;
the rest reproduced in every session.

| Workload | 1.0.1 | 2.0.0 | Change |
|---|---:|---:|---:|
| versioned struct encode | 37.8 | 14.8 | −61 % |
| versioned struct decode | 23.9 | 14.1 | −41 % |
| `BTreeMap<String, u64>` × 100 encode | 3,064 | 1,115 | −64 % |
| `BTreeMap<u32, u32>` × 100 encode | 4,074 | 3,310 | −19 % |
| `HashMap<u64, u64>` × 100 encode | 4,042 | 3,290 | −19 % |
| `[u8; 32]` encode | 14.5 | 5.3 | −64 % |
| `[u8; 32]` decode | 15.3 | 0.8 | −95 % |
| 64 KiB `Vec<u8>` decode | 884 | 554 | −37 % |
| log record decode (owned) | 76.2 | 62.1 | −19 % |
| log record encode | 25.3 | 24.2 | −4 % |
| log record view | 25.8 | 26.6 | +3 % |
| plain struct encode | 14.1 | 15.0 | +6 % *noise* (−3 % to +6 % across sessions) |
| plain struct decode | 12.4 | 12.2 | −1 % |
| `BTreeMap<u32, u32>` × 100 decode | 901 | 893 | −1 % |
| `Vec<u64>` × 1000 decode | 1,068 | 1,196 | **+12 %** in the pre-release tree; fixed before publication (see below) |
| `IoDecoder` 4 KiB `String` | 126 | 125 | −1 % |
| `IoDecoder` `Vec<u64>` × 1000 | 1,274 | 1,272 | 0 % |
| `IoDecoder` `BTreeMap<u32, u32>` × 100 | 849 | 854 | +1 % |
| `IoDecoder` plain struct | 15.5 | 18.8 | **+21 %** (see below) |
| `u64` round-trip | 5.8 | 6.2 | +6 % *noise* (−1 % to +9 % across sessions) |
| 64-byte `String` round-trip | 17.1 | 16.7 | −3 % |
| 64-byte `&str` view round-trip | 13.0 | 13.3 | +3 % |

**Two rows exceeded the 5 % budget in the pre-release tree**, and both
are recorded rather than hidden:

- **`Vec<u64>` decode, +12 %: fixed before publication.** This baseline
  attributed it to code layout. The independent re-audit (PACK-R04)
  showed the cause was the canonical-mode check that every varint read
  performed: removing only that check removed the regression. Integer
  `deserialize_many` now reads the canonical flag once per collection
  and runs a varint loop specialised for it, and scalar varints consult
  the configuration only after a redundant zero byte. Re-measured with
  the re-audit's min-of-trials harness (Windows 11, rustc 1.99, best of
  six alternating sessions × 200 trials), `Vec<u64>` × 1000 decode
  against 1.0.1 is −16 % (multi-byte values) and −19 % (values below
  100) in a default `release` build, and −37 % / −61 % with fat LTO; the
  pre-release tree measured +10 % / +21 % and −0.4 % / −0.1 % there.
- **`IoDecoder` plain struct, +3 ns per struct.** This is the 2.0 depth
  limit: every derived decoder enters and leaves one nesting level. On
  the in-memory decoder the same two integer operations are within
  noise. It is the price of rejecting stack-exhausting input on streams.

Also measured and rejected while preparing 2.0: canonical mode on
`IoDecoder` (recording the raw bytes of map keys while they are read).
Even with the recording path out of line, the extra check on every read
made *lenient* streaming decodes 30–60 % slower, so `IoDecoder` keeps
rejecting canonical mode and the streaming rows above are unchanged.

### Canonical mode cost (opt-in, in-memory decoder)

| Workload | lenient | canonical | Change |
|---|---:|---:|---:|
| `BTreeMap<u32, u32>` × 100 decode | 1,099 | 1,347 | +23 % |
| `BTreeMap<String, u64>` × 100 decode | 4,131 | 4,281 | +4 % |

For arrays of varints the difference was inside this harness's layout
noise (−10 % to +25 % between builds). These numbers are the reason
canonical decoding stays opt-in in 2.0 (see
[`docs/API.md`](./API.md#canonical-decoding)).

## Summary

- **pack-io is the fastest of the four** on the `u64` and 64-byte
  `String` round-trips, the `&str` view (uncontested), and 4 KiB
  `Vec<u8>` decode.
- **Tied:** struct encode with bincode (within 4 %); owned struct decode
  with rkyv (within 9 %), both well ahead of bincode and postcard on this
  payload.
- **Intentional loss:** the zero-copy struct view is about 2× slower than
  rkyv's archived access — rkyv reads a fixed memory layout, pack-io
  walks varints as the wire spec defines.
- 2.0's hardening makes several paths substantially faster and costs
  nothing measurable on most unchanged paths; the two exceptions are
  listed above with their causes.

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
