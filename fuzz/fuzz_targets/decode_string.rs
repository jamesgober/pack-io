//! Fuzz target: arbitrary bytes → `decode::<String>`.
//!
//! Contract: never panic. The codec MAY succeed (the random bytes happened
//! to form a valid varint length + UTF-8 sequence) or MAY fail with any
//! `SerialError` variant. What it must not do is panic, read past `data`,
//! or allocate more than `Config::max_alloc` (1 GiB by default).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<String>(data);
});
