//! Fuzz target: arbitrary bytes → `decode::<HashMap<String, Vec<u8>>>`.
//!
//! Collection paths are the most complex decoders in the crate — they
//! combine varint count parsing, per-entry decode, and `HashMap` insertion.
//! This target exercises the `guard_element_count` + `initial_capacity`
//! caps that bound memory use under hostile inputs.

#![no_main]

use std::collections::HashMap;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<HashMap<String, Vec<u8>>>(data);
});
