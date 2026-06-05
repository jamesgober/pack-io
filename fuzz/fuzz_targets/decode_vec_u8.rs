//! Fuzz target: arbitrary bytes → `decode::<Vec<u8>>`.
//!
//! Exercises the byte-run fast path (`u8::deserialize_many`) that v0.6
//! introduced. The decoder's length-prefix validation against
//! `Config::max_alloc` is the first line of defence against hostile inputs.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<Vec<u8>>(data);
});
