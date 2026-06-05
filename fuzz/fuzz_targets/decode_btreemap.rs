//! Fuzz target: arbitrary bytes → `decode::<BTreeMap<u64, String>>`.
//!
//! Mirrors the existing `decode_collection` target but exercises the
//! `BTreeMap` (non-`std`-gated, ordered) path instead of the `HashMap`
//! (`std`-gated, randomised) path. Different `K`/`V` types so the
//! varint-count cap + per-entry decode is hit through a fresh
//! monomorphisation.

#![no_main]

use std::collections::BTreeMap;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<BTreeMap<u64, String>>(data);
});
