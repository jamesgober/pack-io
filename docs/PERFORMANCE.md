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
> ecosystem — `bincode`, `postcard`, and `rkyv` — and the analysis behind
> them. The numbers live in
> [`docs/PERFORMANCE_BASELINE.md`](./PERFORMANCE_BASELINE.md).

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
- **`black_box`** on inputs and outputs so the optimiser cannot fold the
  encode / decode away.
- For **rkyv** both the owned-deserialize path (compare with pack-io's
  `decode`) and the archived-access path (compare with `decode_view`)
  are measured.
- **Two harnesses.** [`benches/comparative.rs`](../benches/comparative.rs)
  (Criterion) is the right tool on an idle machine.
  [`benches/min_trials.rs`](../benches/min_trials.rs) takes the fastest
  of 300 short trials and is the right tool on a shared one; it produced
  the v2.0 baseline because the measurement machine was busy (see the
  baseline file for the details and the environment).

## Summary — wins, ties, losses (v2.0.0, Linux x86_64)

- **Wins (fastest of the four):** `u64` round-trip (5.8 ns vs postcard
  6.7, bincode 8.2), 64-byte `String` round-trip (17.1 vs 21.0), 4 KiB
  `Vec<u8>` decode (33.6 vs 52.2), and the zero-copy `&str` view (5.2 ns,
  uncontested — bincode and postcard have no zero-copy path).
- **Ties:** struct encode with bincode (27.2 vs 26.2 ns); owned struct
  decode with rkyv (63.2 vs 58.0 ns). On this payload and allocator
  both are well ahead of bincode's owned decode (168 ns).
- **Loss:** the zero-copy struct view (25.1 ns) against rkyv's archived
  access (13.6 ns). See below.

Results depend on the platform and allocator. On the Windows host, in
the 1.0 Criterion runs, bincode's owned decode was within a few percent
of pack-io's rather than 2.7× slower, and pack-io's `u64` round-trip was
about 8 % behind bincode's. The ordering of encode, `Vec<u8>` decode and
the views was the same on both. Benchmark your own workload on your own
target before relying on a ratio.

## Honest losses

1. **`decode_view` against rkyv's archived access (about 2×).** rkyv's
   archive is a fixed memory layout: there are no varints to decode and
   no length prefixes to walk. pack-io's view path still walks the
   varint stream, validates every length prefix against the input and
   `max_alloc`, and checks UTF-8. Matching rkyv would mean shipping an
   archive format, and the whole wire format is built on varints
   precisely so a third party can implement it from a one-page spec. The
   trade-off is intentional.

## What changed in v2.0

The 2.0 hardening (input-bounded allocation, depth limit, config
propagation, canonical mode) was designed to cost nothing on the paths it
does not change, and a few paths were rebuilt:

1. **Versioned structs** compute the body length with
   `Serialize::encoded_len` and write their fields straight into the
   output instead of encoding into a temporary `Vec` and copying it
   (encode −61 %). On decode the in-memory decoder borrows the body
   instead of copying it into a fresh buffer (−41 %).
2. **Maps and sets** encode every entry into one shared scratch buffer
   with recorded spans instead of one `Vec` per entry, and skip the sort
   when entries already arrive in canonical order (`BTreeMap<String, _>`
   −64 %, integer-keyed maps −19 %).
3. **`[u8; N]`** encodes as one bulk write and decodes into a stack
   array with no heap allocation (decode −95 %).
4. **`Vec<u8>` decode** copies straight from the input after the bounds
   check instead of zero-filling a buffer first (−37 % at 64 KiB).
5. **Canonical mode is free when off.** The varint decode loop is
   instantiated twice at compile time; the redundant-byte check exists
   only in the canonical instance. (A first version that checked the
   flag inside the shared loop cost about 19 % on `Vec<u64>` decode and
   was replaced.) When on, it costs about 23 % on small-key map decoding
   and about 4 % with string keys, which is why it stays opt-in.
6. **Depth tracking** is two integer operations per derived value. The
   generated decode body runs inside a labelled block rather than a
   closure. It is within noise on the in-memory decoder and about 3 ns
   per struct on `IoDecoder`.
7. **Streaming canonical mode was tried and dropped.** Recording map
   keys inside `IoDecoder` cost 30–60 % on *lenient* streaming decodes,
   even with the recording path out of line, so streams keep rejecting
   canonical mode (buffer the frame instead).

Known issue: `Vec<u64>` decode measures about 12 % slower than 1.0.1 in
this harness although its loop is unchanged; see the baseline file.

`encode()` keeps its 512-byte initial capacity: measured against sizing
the buffer exactly with `encoded_len`, it is the faster choice for the
short-lived buffers it is meant for. Long-lived values should be encoded
into `Encoder::with_capacity(value.encoded_len()?)`.

## Reproducing locally

```bash
# Comparative, robust on a busy machine (about 1 minute):
cargo bench --bench min_trials --features derive

# Comparative, Criterion (idle machine; about 4 minutes):
cargo bench --bench comparative --features derive -- --warm-up-time 2 --measurement-time 10

# Hot paths touched in 2.0 (uses only 1.0 API, so it also runs on 1.0.1):
cargo bench --bench hot_paths --features derive

# Canonical decode, serde adapter, encoded_len:
cargo bench --bench features --features derive,serde
```

Criterion writes detailed reports to `target/criterion/`.

## Per-codec footprint note

The dev-dependencies for the comparative benchmarks (`bincode 2.0.1`,
`postcard 1`, `rkyv 0.8.18`, `serde 1`) are benchmark fixtures only —
they never enter the published `pack-io` crate. `pack-io` has zero
required dependencies in the default `std` build; it gains
`pack-io-derive` (and transitively `proc-macro2`, `quote`, `syn`) with
`derive`, `serde` with `serde`, and `bytes` with `bytes`.

---

<sub>Copyright &copy; 2026 <strong>James Gober</strong>. All rights reserved.</sub>
