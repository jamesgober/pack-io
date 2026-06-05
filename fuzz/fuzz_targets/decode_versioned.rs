//! Fuzz target: arbitrary bytes → versioned struct `Deserialize`.
//!
//! Exercises the schema-evolution decode path:
//! `varint(version) ++ varint(body_len) ++ body`. The body-length cap
//! against `Config::max_alloc` is the primary defence against hostile
//! payloads that declare gigabyte-class bodies.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pack_io::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[pack_io(version = 2)]
struct Message {
    id: u64,
    text: String,
    #[pack_io(since = 2)]
    timestamp: Option<u64>,
}

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<Message>(data);
});
