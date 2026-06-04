//! Codec micro-benchmarks.
//!
//! ## What this file covers (and what it doesn't)
//!
//! These benchmarks measure pack-io against itself — the headline contract
//! is "View<T> decode is measurably faster than owning decode on
//! borrow-heavy types", and this is where we prove it. Comparative
//! benchmarks against `bincode` / `postcard` / `rkyv` belong in a separate
//! suite that gets stood up before the first crates.io publish so the
//! numbers we cite in the README are honest.
//!
//! Run with:
//!
//! ```bash
//! cargo bench --bench codec_bench
//! ```

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use pack_io::{Deserialize, DeserializeView, Serialize, decode, decode_view, encode};

// ---------------------------------------------------------------------------
// Representative payload: a borrow-heavy "log line"-shaped record.
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
struct OwnedRecord {
    timestamp: u64,
    level: u8,
    message: String,
    tags: Vec<String>,
    payload: Vec<u8>,
}

#[derive(DeserializeView)]
#[allow(dead_code)] // every field is decoded; benches inspect a subset via black_box
struct ViewRecord<'a> {
    timestamp: u64,
    level: u8,
    message: &'a str,
    tags: Vec<&'a str>,
    payload: &'a [u8],
}

fn make_record() -> OwnedRecord {
    OwnedRecord {
        timestamp: 1_700_000_000,
        level: 4,
        message: "request handled successfully in 2.3ms".into(),
        tags: vec![
            "http".into(),
            "200".into(),
            "us-east-1".into(),
            "service:api".into(),
        ],
        payload: vec![0xab; 256],
    }
}

// ---------------------------------------------------------------------------
// Benches
// ---------------------------------------------------------------------------

fn bench_encode(c: &mut Criterion) {
    let record = make_record();
    c.bench_function("encode/owned_record", |b| {
        b.iter(|| {
            let bytes = encode(black_box(&record)).unwrap();
            black_box(bytes);
        });
    });
}

fn bench_decode_owned(c: &mut Criterion) {
    let bytes = encode(&make_record()).unwrap();
    c.bench_function("decode/owned_record", |b| {
        b.iter(|| {
            let r: OwnedRecord = decode(black_box(&bytes)).unwrap();
            black_box(r);
        });
    });
}

fn bench_decode_view(c: &mut Criterion) {
    let bytes = encode(&make_record()).unwrap();
    c.bench_function("decode/view_record", |b| {
        b.iter(|| {
            let r: ViewRecord<'_> = decode_view(black_box(&bytes)).unwrap();
            black_box(r.message);
        });
    });
}

fn bench_encode_decode_primitive(c: &mut Criterion) {
    let mut group = c.benchmark_group("primitive_round_trip");
    group.bench_function("u64", |b| {
        b.iter(|| {
            let bytes = encode(black_box(&42_u64)).unwrap();
            let back: u64 = decode(&bytes).unwrap();
            black_box(back);
        });
    });
    group.bench_function("string_64_bytes", |b| {
        let s: String = (0..64).map(|_| 'x').collect();
        b.iter(|| {
            let bytes = encode(black_box(&s)).unwrap();
            let back: String = decode(&bytes).unwrap();
            black_box(back);
        });
    });
    group.bench_function("string_view_64_bytes", |b| {
        let s: String = (0..64).map(|_| 'x').collect();
        let bytes = encode(&s).unwrap();
        b.iter(|| {
            let back: &str = decode_view(black_box(&bytes)).unwrap();
            black_box(back);
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_encode,
    bench_decode_owned,
    bench_decode_view,
    bench_encode_decode_primitive,
);
criterion_main!(benches);
