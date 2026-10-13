//! Benchmarks for the 2.0 features: canonical-mode decode cost, the serde
//! adapter against the native derive, and `encoded_len`.
//!
//! Run with:
//!
//! ```bash
//! cargo bench --bench features --features derive,serde
//! ```

use std::collections::BTreeMap;
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use pack_io::{Config, Serialize, decode, decode_with_config, encode};

#[derive(Clone, pack_io::Serialize, pack_io::Deserialize, serde::Serialize, serde::Deserialize)]
struct Record {
    id: u64,
    name: String,
    tags: Vec<String>,
    scores: BTreeMap<String, i64>,
    payload: Vec<u8>,
}

fn record() -> Record {
    Record {
        id: 1_700_000_000,
        name: "benchmark-record".into(),
        tags: (0..8).map(|i| format!("tag-{i}")).collect(),
        scores: (0..32)
            .map(|i| (format!("k{i:03}"), i * 1000 - 7))
            .collect(),
        payload: vec![0x5a; 256],
    }
}

fn bench_canonical(c: &mut Criterion) {
    let r = record();
    let bytes = encode(&r).unwrap();
    let strict = Config::new().with_canonical(true);
    let mut group = c.benchmark_group("canonical");
    group.bench_function("lenient_decode", |b| {
        b.iter(|| black_box(decode::<Record>(black_box(&bytes)).unwrap()));
    });
    group.bench_function("canonical_decode", |b| {
        b.iter(|| black_box(decode_with_config::<Record>(black_box(&bytes), strict).unwrap()));
    });
    group.finish();
}

fn bench_serde(c: &mut Criterion) {
    let r = record();
    let bytes = encode(&r).unwrap();
    let mut group = c.benchmark_group("serde_vs_native");
    group.bench_function("native_encode", |b| {
        b.iter(|| black_box(encode(black_box(&r)).unwrap()));
    });
    group.bench_function("serde_encode", |b| {
        b.iter(|| black_box(pack_io::serde::to_vec(black_box(&r)).unwrap()));
    });
    group.bench_function("native_decode", |b| {
        b.iter(|| black_box(decode::<Record>(black_box(&bytes)).unwrap()));
    });
    group.bench_function("serde_decode", |b| {
        b.iter(|| black_box(pack_io::serde::from_slice::<Record>(black_box(&bytes)).unwrap()));
    });
    group.finish();
}

fn bench_encoded_len(c: &mut Criterion) {
    let r = record();
    let mut group = c.benchmark_group("encoded_len");
    group.bench_function("record", |b| {
        b.iter(|| black_box(black_box(&r).encoded_len().unwrap()));
    });
    group.bench_function("record_exact_encode", |b| {
        b.iter(|| {
            let r = black_box(&r);
            let mut enc = pack_io::Encoder::with_capacity(r.encoded_len().unwrap());
            r.serialize(&mut enc).unwrap();
            black_box(enc.into_inner())
        });
    });
    group.finish();
}

criterion_group!(benches, bench_canonical, bench_serde, bench_encoded_len);
criterion_main!(benches);
