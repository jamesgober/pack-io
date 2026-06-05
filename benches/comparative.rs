//! Comparative benchmarks vs `bincode`, `postcard`, and `rkyv`.
//!
//! The numbers from this file back the "Speed ✓" claim in the README. They
//! are reproducible: `cargo bench --bench comparative --features derive`.
//!
//! ## Methodology
//!
//! - **Same logical payload** for every codec. Each crate gets a struct
//!   shape with its own native derives, so each is on its preferred path.
//! - **Default integer encoding** for each codec — `bincode` and `postcard`
//!   both use varints in their defaults, matching pack-io. `rkyv` uses a
//!   fixed-layout archive (no varints).
//! - **`black_box`** on both inputs and outputs so the optimiser can't fold
//!   the encode/decode away.
//! - For **rkyv** we measure both the owned-deserialize path (apples-to-
//!   apples with pack-io's `decode`) and the archived-access path (apples-
//!   to-apples with pack-io's `decode_view`).

use std::hint::black_box;

use bincode::{Decode as BcDecode, Encode as BcEncode};
use criterion::{Criterion, criterion_group, criterion_main};
use pack_io::{
    Deserialize as PiDeserialize, DeserializeView as PiDeserializeView, Serialize as PiSerialize,
    decode as pi_decode, decode_view as pi_decode_view, encode as pi_encode,
};
use rkyv::rancor::Error as RkyvError;
use serde::{Deserialize as SerdeDe, Serialize as SerdeSer};

// ---------------------------------------------------------------------------
// Shared payload — a borrow-heavy "log line" record.
// ---------------------------------------------------------------------------

#[derive(
    Clone,
    PiSerialize,
    PiDeserialize,
    BcEncode,
    BcDecode,
    SerdeSer,
    SerdeDe,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
struct LogRecord {
    timestamp: u64,
    level: u8,
    message: String,
    tags: Vec<String>,
    payload: Vec<u8>,
}

#[derive(PiDeserializeView)]
struct LogRecordView<'a> {
    #[allow(dead_code)]
    timestamp: u64,
    #[allow(dead_code)]
    level: u8,
    message: &'a str,
    #[allow(dead_code)]
    tags: Vec<&'a str>,
    #[allow(dead_code)]
    payload: &'a [u8],
}

fn make_record() -> LogRecord {
    LogRecord {
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
// Encode benches
// ---------------------------------------------------------------------------

fn bench_encode_record(c: &mut Criterion) {
    let record = make_record();
    let bincode_cfg = bincode::config::standard();

    let mut group = c.benchmark_group("encode/log_record");

    group.bench_function("pack_io", |b| {
        b.iter(|| {
            let bytes = pi_encode(black_box(&record)).unwrap();
            black_box(bytes);
        });
    });

    group.bench_function("bincode", |b| {
        b.iter(|| {
            let bytes = bincode::encode_to_vec(black_box(&record), bincode_cfg).unwrap();
            black_box(bytes);
        });
    });

    group.bench_function("postcard", |b| {
        b.iter(|| {
            let bytes: Vec<u8> = postcard::to_allocvec(black_box(&record)).unwrap();
            black_box(bytes);
        });
    });

    group.bench_function("rkyv", |b| {
        b.iter(|| {
            let bytes = rkyv::to_bytes::<RkyvError>(black_box(&record)).unwrap();
            black_box(bytes);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Owning decode benches — pack-io's `decode` vs each competitor's owned read.
// ---------------------------------------------------------------------------

fn bench_decode_owned_record(c: &mut Criterion) {
    let record = make_record();
    let bincode_cfg = bincode::config::standard();

    let pi_bytes = pi_encode(&record).unwrap();
    let bincode_bytes = bincode::encode_to_vec(&record, bincode_cfg).unwrap();
    let postcard_bytes: Vec<u8> = postcard::to_allocvec(&record).unwrap();
    let rkyv_bytes = rkyv::to_bytes::<RkyvError>(&record).unwrap();

    let mut group = c.benchmark_group("decode_owned/log_record");

    group.bench_function("pack_io", |b| {
        b.iter(|| {
            let r: LogRecord = pi_decode(black_box(&pi_bytes)).unwrap();
            black_box(r);
        });
    });

    group.bench_function("bincode", |b| {
        b.iter(|| {
            let (r, _): (LogRecord, usize) =
                bincode::decode_from_slice(black_box(&bincode_bytes), bincode_cfg).unwrap();
            black_box(r);
        });
    });

    group.bench_function("postcard", |b| {
        b.iter(|| {
            let r: LogRecord = postcard::from_bytes(black_box(&postcard_bytes)).unwrap();
            black_box(r);
        });
    });

    group.bench_function("rkyv", |b| {
        b.iter(|| {
            let archived =
                rkyv::access::<ArchivedLogRecord, RkyvError>(black_box(&rkyv_bytes[..])).unwrap();
            let r: LogRecord = rkyv::deserialize::<LogRecord, RkyvError>(archived).unwrap();
            black_box(r);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Zero-copy / archived decode benches — pack-io's View vs rkyv's archived
// access. bincode and postcard have no zero-copy story, so they are absent
// from this group by design.
// ---------------------------------------------------------------------------

fn bench_decode_view_record(c: &mut Criterion) {
    let record = make_record();
    let pi_bytes = pi_encode(&record).unwrap();
    let rkyv_bytes = rkyv::to_bytes::<RkyvError>(&record).unwrap();

    let mut group = c.benchmark_group("decode_view/log_record");

    group.bench_function("pack_io_view", |b| {
        b.iter(|| {
            let view: LogRecordView<'_> = pi_decode_view(black_box(&pi_bytes)).unwrap();
            black_box(view.message);
        });
    });

    group.bench_function("rkyv_archived", |b| {
        b.iter(|| {
            let archived =
                rkyv::access::<ArchivedLogRecord, RkyvError>(black_box(&rkyv_bytes[..])).unwrap();
            black_box(&archived.message);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Primitive round trips — u64 and a 64-byte String. The headline
// "encode + decode in <10ns per byte" target lives here.
// ---------------------------------------------------------------------------

fn bench_primitive_u64(c: &mut Criterion) {
    let value: u64 = 1_234_567_890;
    let bincode_cfg = bincode::config::standard();
    let mut group = c.benchmark_group("u64_round_trip");

    group.bench_function("pack_io", |b| {
        b.iter(|| {
            let bytes = pi_encode(black_box(&value)).unwrap();
            let back: u64 = pi_decode(&bytes).unwrap();
            black_box(back);
        });
    });

    group.bench_function("bincode", |b| {
        b.iter(|| {
            let bytes = bincode::encode_to_vec(black_box(&value), bincode_cfg).unwrap();
            let (back, _): (u64, usize) = bincode::decode_from_slice(&bytes, bincode_cfg).unwrap();
            black_box(back);
        });
    });

    group.bench_function("postcard", |b| {
        b.iter(|| {
            let bytes: Vec<u8> = postcard::to_allocvec(black_box(&value)).unwrap();
            let back: u64 = postcard::from_bytes(&bytes).unwrap();
            black_box(back);
        });
    });

    group.finish();
}

fn bench_primitive_string_64(c: &mut Criterion) {
    let s: String = "x".repeat(64);
    let bincode_cfg = bincode::config::standard();
    let mut group = c.benchmark_group("string64_round_trip");

    group.bench_function("pack_io_owning", |b| {
        b.iter(|| {
            let bytes = pi_encode(black_box(&s)).unwrap();
            let back: String = pi_decode(&bytes).unwrap();
            black_box(back);
        });
    });

    group.bench_function("pack_io_view", |b| {
        let bytes = pi_encode(&s).unwrap();
        b.iter(|| {
            let back: &str = pi_decode_view(black_box(&bytes)).unwrap();
            black_box(back);
        });
    });

    group.bench_function("bincode_owning", |b| {
        b.iter(|| {
            let bytes = bincode::encode_to_vec(black_box(&s), bincode_cfg).unwrap();
            let (back, _): (String, usize) =
                bincode::decode_from_slice(&bytes, bincode_cfg).unwrap();
            black_box(back);
        });
    });

    group.bench_function("postcard_owning", |b| {
        b.iter(|| {
            let bytes: Vec<u8> = postcard::to_allocvec(black_box(&s)).unwrap();
            let back: String = postcard::from_bytes(&bytes).unwrap();
            black_box(back);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Byte run — owning decode of a 4 KiB Vec<u8>. Stresses the length-prefixed
// byte-run hot path that the v0.3 generic Vec<T> refactor regressed.
// ---------------------------------------------------------------------------

fn bench_vec_u8_4kib(c: &mut Criterion) {
    let buf: Vec<u8> = vec![0xab; 4096];
    let bincode_cfg = bincode::config::standard();

    let pi_bytes = pi_encode(&buf).unwrap();
    let bincode_bytes = bincode::encode_to_vec(&buf, bincode_cfg).unwrap();
    let postcard_bytes: Vec<u8> = postcard::to_allocvec(&buf).unwrap();

    let mut group = c.benchmark_group("vec_u8_4kib_decode");

    group.bench_function("pack_io", |b| {
        b.iter(|| {
            let back: Vec<u8> = pi_decode(black_box(&pi_bytes)).unwrap();
            black_box(back);
        });
    });

    group.bench_function("bincode", |b| {
        b.iter(|| {
            let (back, _): (Vec<u8>, usize) =
                bincode::decode_from_slice(black_box(&bincode_bytes), bincode_cfg).unwrap();
            black_box(back);
        });
    });

    group.bench_function("postcard", |b| {
        b.iter(|| {
            let back: Vec<u8> = postcard::from_bytes(black_box(&postcard_bytes)).unwrap();
            black_box(back);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_encode_record,
    bench_decode_owned_record,
    bench_decode_view_record,
    bench_primitive_u64,
    bench_primitive_string_64,
    bench_vec_u8_4kib,
);
criterion_main!(benches);
