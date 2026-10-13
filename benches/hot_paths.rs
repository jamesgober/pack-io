//! Hot-path micro-benchmarks for the code paths touched by the 2.0 hardening
//! pass.
//!
//! Every benchmark here uses only API that already existed in 1.0.x, so the
//! same file runs against the 1.0.1 baseline and the current tree. That is
//! what makes the before/after numbers in the release notes comparable.
//!
//! Run with:
//!
//! ```bash
//! cargo bench --bench hot_paths --features derive
//! ```

use std::collections::{BTreeMap, HashMap};
use std::hint::black_box;
use std::io::Cursor;

use criterion::{Criterion, criterion_group, criterion_main};
use pack_io::{Deserialize, IoDecoder, Serialize, decode, encode};

#[derive(Serialize, Deserialize, Clone)]
#[pack_io(version = 3)]
struct Versioned {
    id: u64,
    name: String,
    #[pack_io(since = 2)]
    flags: u32,
    #[pack_io(since = 3)]
    score: i64,
}

#[derive(Serialize, Deserialize, Clone)]
struct Plain {
    id: u64,
    name: String,
    flags: u32,
    score: i64,
}

fn versioned() -> Versioned {
    Versioned {
        id: 1_700_000_000,
        name: "hot-path-record".into(),
        flags: 0b1011,
        score: -42,
    }
}

fn plain() -> Plain {
    Plain {
        id: 1_700_000_000,
        name: "hot-path-record".into(),
        flags: 0b1011,
        score: -42,
    }
}

fn bench_structs(c: &mut Criterion) {
    let mut group = c.benchmark_group("struct");
    let v = versioned();
    let p = plain();
    let v_bytes = encode(&v).unwrap();
    let p_bytes = encode(&p).unwrap();
    group.bench_function("versioned_encode", |b| {
        b.iter(|| black_box(encode(black_box(&v)).unwrap()));
    });
    group.bench_function("versioned_decode", |b| {
        b.iter(|| black_box(decode::<Versioned>(black_box(&v_bytes)).unwrap()));
    });
    group.bench_function("plain_encode", |b| {
        b.iter(|| black_box(encode(black_box(&p)).unwrap()));
    });
    group.bench_function("plain_decode", |b| {
        b.iter(|| black_box(decode::<Plain>(black_box(&p_bytes)).unwrap()));
    });
    group.finish();
}

fn bench_maps(c: &mut Criterion) {
    let mut group = c.benchmark_group("map_100");
    let ints: BTreeMap<u32, u32> = (0..100u32).map(|i| (i * 7919 % 1000, i)).collect();
    let strs: BTreeMap<String, u64> = (0..100u64).map(|i| (format!("key-{i:04}"), i)).collect();
    let hashed: HashMap<u64, u64> = (0..100u64).map(|i| (i * 2_654_435_761, i)).collect();
    let ints_bytes = encode(&ints).unwrap();
    group.bench_function("btreemap_u32_encode", |b| {
        b.iter(|| black_box(encode(black_box(&ints)).unwrap()));
    });
    group.bench_function("btreemap_string_encode", |b| {
        b.iter(|| black_box(encode(black_box(&strs)).unwrap()));
    });
    group.bench_function("hashmap_u64_encode", |b| {
        b.iter(|| black_box(encode(black_box(&hashed)).unwrap()));
    });
    group.bench_function("btreemap_u32_decode", |b| {
        b.iter(|| black_box(decode::<BTreeMap<u32, u32>>(black_box(&ints_bytes)).unwrap()));
    });
    group.finish();
}

fn bench_bytes(c: &mut Criterion) {
    let mut group = c.benchmark_group("bytes");
    let small = encode(&7u8).unwrap();
    let arr = [0xa5u8; 32];
    let arr_bytes = encode(&arr).unwrap();
    let blob: Vec<u8> = vec![0x5a; 64 * 1024];
    let blob_bytes = encode(&blob).unwrap();
    let text: String = "y".repeat(4096);
    let text_bytes = encode(&text).unwrap();
    let ints: Vec<u64> = (0..1000u64).map(|i| i * i * 31).collect();
    let ints_bytes = encode(&ints).unwrap();

    group.bench_function("encode_u8", |b| {
        b.iter(|| black_box(encode(black_box(&7u8)).unwrap()));
    });
    group.bench_function("decode_u8", |b| {
        b.iter(|| black_box(decode::<u8>(black_box(&small)).unwrap()));
    });
    group.bench_function("array_u8_32_encode", |b| {
        b.iter(|| black_box(encode(black_box(&arr)).unwrap()));
    });
    group.bench_function("array_u8_32_decode", |b| {
        b.iter(|| black_box(decode::<[u8; 32]>(black_box(&arr_bytes)).unwrap()));
    });
    group.bench_function("vec_u8_64kib_decode", |b| {
        b.iter(|| black_box(decode::<Vec<u8>>(black_box(&blob_bytes)).unwrap()));
    });
    group.bench_function("vec_u64_1000_decode", |b| {
        b.iter(|| black_box(decode::<Vec<u64>>(black_box(&ints_bytes)).unwrap()));
    });
    group.bench_function("io_string_4kib_decode", |b| {
        b.iter(|| {
            let mut dec = IoDecoder::new(Cursor::new(black_box(&text_bytes[..])));
            black_box(dec.read::<String>().unwrap())
        });
    });
    group.bench_function("io_vec_u8_64kib_decode", |b| {
        b.iter(|| {
            let mut dec = IoDecoder::new(Cursor::new(black_box(&blob_bytes[..])));
            black_box(dec.read::<Vec<u8>>().unwrap())
        });
    });
    group.finish();
}

criterion_group!(benches, bench_structs, bench_maps, bench_bytes);
criterion_main!(benches);
