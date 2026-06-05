//! Fuzz target: arbitrary bytes → `decode_view::<&str>`.
//!
//! Zero-copy decode path. The contract: the returned `&str` is borrowed
//! from `data`, the borrow checker enforces it cannot outlive `data`, and
//! the UTF-8 validation step must reject any invalid byte sequence.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode_view::<&str>(data);
});
