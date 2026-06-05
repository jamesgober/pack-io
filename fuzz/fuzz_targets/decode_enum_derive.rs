//! Fuzz target: arbitrary bytes → derive-generated enum `Deserialize`.
//!
//! Exercises the variant-index varint + per-variant decode path. The
//! `UnknownVariant` error variant should fire on any tag outside the
//! declared range, never a panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pack_io::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
enum Event {
    Heartbeat,
    Login { user: u64, ip: String },
    Error(u32, String),
    Disconnect { reason: Option<String> },
}

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<Event>(data);
});
