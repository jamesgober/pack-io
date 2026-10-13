//! The workloads of `benches/comparative.rs`, timed as the minimum over many
//! short trials instead of Criterion's mean.
//!
//! On a machine that is busy with other work, Criterion's estimates move by
//! tens of percent between runs (every codec slows down together). The
//! fastest of 300 trials of 2,000 iterations each is close to the
//! uncontended cost and much more stable, which is what the published
//! tables in `docs/PERFORMANCE_BASELINE.md` use. Pin it to one core for the
//! best repeatability:
//!
//! ```bash
//! cargo bench --bench min_trials --features derive            # any OS
//! taskset -c 7 target/release/deps/min_trials-*                 # Linux, pinned
//! ```
#![allow(clippy::unwrap_used)]
use std::hint::black_box;
use std::time::Instant;

use bincode::{Decode as BcDecode, Encode as BcEncode};
use rkyv::rancor::Error as RkyvError;

#[derive(
    Clone,
    pack_io::Serialize,
    pack_io::Deserialize,
    BcEncode,
    BcDecode,
    serde::Serialize,
    serde::Deserialize,
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

#[derive(pack_io::DeserializeView)]
#[allow(dead_code)]
struct LogRecordView<'a> {
    timestamp: u64,
    level: u8,
    message: &'a str,
    tags: Vec<&'a str>,
    payload: &'a [u8],
}

fn time<F: FnMut()>(mut f: F) -> f64 {
    const ITERS: u32 = 2_000;
    let mut best = f64::MAX;
    for _ in 0..300 {
        let t = Instant::now();
        for _ in 0..ITERS {
            f();
        }
        best = best.min(t.elapsed().as_nanos() as f64 / f64::from(ITERS));
    }
    best
}

fn main() {
    let record = LogRecord {
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
    };
    let cfg = bincode::config::standard();
    let pi = pack_io::encode(&record).unwrap();
    let bc = bincode::encode_to_vec(&record, cfg).unwrap();
    let pc = postcard::to_allocvec(&record).unwrap();
    let rk = rkyv::to_bytes::<RkyvError>(&record).unwrap();
    let s64 = "x".repeat(64);
    let s64_bytes = pack_io::encode(&s64).unwrap();
    let blob = vec![0xabu8; 4096];
    let pi_blob = pack_io::encode(&blob).unwrap();
    let bc_blob = bincode::encode_to_vec(&blob, cfg).unwrap();
    let pc_blob = postcard::to_allocvec(&blob).unwrap();

    let rows: Vec<(&str, f64)> = vec![
        (
            "encode/pack_io",
            time(|| {
                black_box(pack_io::encode(black_box(&record)).unwrap());
            }),
        ),
        (
            "encode/bincode",
            time(|| {
                black_box(bincode::encode_to_vec(black_box(&record), cfg).unwrap());
            }),
        ),
        (
            "encode/postcard",
            time(|| {
                black_box(postcard::to_allocvec(black_box(&record)).unwrap());
            }),
        ),
        (
            "encode/rkyv",
            time(|| {
                black_box(rkyv::to_bytes::<RkyvError>(black_box(&record)).unwrap());
            }),
        ),
        (
            "decode/pack_io",
            time(|| {
                black_box(pack_io::decode::<LogRecord>(black_box(&pi)).unwrap());
            }),
        ),
        (
            "decode/bincode",
            time(|| {
                black_box(bincode::decode_from_slice::<LogRecord, _>(black_box(&bc), cfg).unwrap());
            }),
        ),
        (
            "decode/postcard",
            time(|| {
                black_box(postcard::from_bytes::<LogRecord>(black_box(&pc)).unwrap());
            }),
        ),
        (
            "decode/rkyv",
            time(|| {
                let a = rkyv::access::<ArchivedLogRecord, RkyvError>(black_box(&rk[..])).unwrap();
                black_box(rkyv::deserialize::<LogRecord, RkyvError>(a).unwrap());
            }),
        ),
        (
            "view/pack_io",
            time(|| {
                black_box(
                    pack_io::decode_view::<LogRecordView<'_>>(black_box(&pi))
                        .unwrap()
                        .message
                        .len(),
                );
            }),
        ),
        (
            "view/rkyv",
            time(|| {
                black_box(
                    rkyv::access::<ArchivedLogRecord, RkyvError>(black_box(&rk[..]))
                        .unwrap()
                        .message
                        .len(),
                );
            }),
        ),
        (
            "u64/pack_io",
            time(|| {
                let b = pack_io::encode(black_box(&42u64)).unwrap();
                black_box(pack_io::decode::<u64>(&b).unwrap());
            }),
        ),
        (
            "u64/bincode",
            time(|| {
                let b = bincode::encode_to_vec(black_box(&42u64), cfg).unwrap();
                black_box(bincode::decode_from_slice::<u64, _>(&b, cfg).unwrap());
            }),
        ),
        (
            "u64/postcard",
            time(|| {
                let b = postcard::to_allocvec(black_box(&42u64)).unwrap();
                black_box(postcard::from_bytes::<u64>(&b).unwrap());
            }),
        ),
        (
            "string64/pack_io",
            time(|| {
                let b = pack_io::encode(black_box(&s64)).unwrap();
                black_box(pack_io::decode::<String>(&b).unwrap());
            }),
        ),
        (
            "string64/bincode",
            time(|| {
                let b = bincode::encode_to_vec(black_box(&s64), cfg).unwrap();
                black_box(bincode::decode_from_slice::<String, _>(&b, cfg).unwrap());
            }),
        ),
        (
            "string64/postcard",
            time(|| {
                let b = postcard::to_allocvec(black_box(&s64)).unwrap();
                black_box(postcard::from_bytes::<String>(&b).unwrap());
            }),
        ),
        (
            "string64_view/pack_io",
            time(|| {
                black_box(
                    pack_io::decode_view::<&str>(black_box(&s64_bytes))
                        .unwrap()
                        .len(),
                );
            }),
        ),
        (
            "vec4k/pack_io",
            time(|| {
                black_box(pack_io::decode::<Vec<u8>>(black_box(&pi_blob)).unwrap());
            }),
        ),
        (
            "vec4k/bincode",
            time(|| {
                black_box(
                    bincode::decode_from_slice::<Vec<u8>, _>(black_box(&bc_blob), cfg).unwrap(),
                );
            }),
        ),
        (
            "vec4k/postcard",
            time(|| {
                black_box(postcard::from_bytes::<Vec<u8>>(black_box(&pc_blob)).unwrap());
            }),
        ),
    ];
    println!("{:<24} {:>10}", "benchmark", "ns/iter");
    for (name, ns) in rows {
        println!("{name:<24} {ns:>10.2}");
    }
}
