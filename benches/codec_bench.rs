//! Benchmark harness for the codec hot paths.
//!
//! The actual encode/decode benchmarks land alongside the codec implementation
//! in the `v0.2` foundation release. This placeholder keeps the bench target
//! configured in `Cargo.toml` building cleanly so the CI workflow does not
//! need a Cargo.toml edit at every roadmap phase.

use criterion::{Criterion, criterion_group, criterion_main};

fn version_constant(c: &mut Criterion) {
    c.bench_function("pack_io::VERSION", |b| {
        b.iter(|| std::hint::black_box(pack_io::VERSION));
    });
}

criterion_group!(benches, version_constant);
criterion_main!(benches);
