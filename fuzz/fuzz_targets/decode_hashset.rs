//! Fuzz target: arbitrary bytes → `decode::<HashSet<u32>>`.
//!
//! Hash-set path — `std`-gated, exercises the `with_capacity_and_hasher`
//! preallocation cap and the per-element `insert`.

#![no_main]

use std::collections::HashSet;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<HashSet<u32>>(data);
});
