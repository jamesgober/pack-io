//! Fuzz target: arbitrary bytes → `decode::<BTreeSet<String>>`.
//!
//! Covers the ordered-set decode path with a `String` element type so
//! the varint length prefix + UTF-8 validation are inside the
//! per-element decode loop.

#![no_main]

use std::collections::BTreeSet;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<BTreeSet<String>>(data);
});
