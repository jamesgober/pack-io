//! Fuzz target: arbitrary bytes → derive-generated `Deserialize` for a
//! representative struct shape.
//!
//! Exercises the codegen path emitted by `#[derive(pack_io::Deserialize)]`
//! — same shape used in the comparative benchmark. The decoder must
//! handle any byte sequence without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pack_io::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct LogRecord {
    timestamp: u64,
    level: u8,
    message: String,
    tags: Vec<String>,
    payload: Vec<u8>,
}

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<LogRecord>(data);
});
