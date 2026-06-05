//! Fuzz target: arbitrary bytes → `decode::<(u64, String, Vec<u8>)>`.
//!
//! Mixed primitive + length-prefixed shape — exercises the tuple
//! deserialisation path plus every primitive impl in sequence.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<(u64, String, Vec<u8>)>(data);
});
