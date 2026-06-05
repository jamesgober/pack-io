//! Fuzz target: arbitrary bytes → `decode_view::<&[u8]>`.
//!
//! Zero-copy `&[u8]` decode path. Same length-prefix + borrow
//! validation as `decode_view::<&str>`, minus the UTF-8 check —
//! exercises the `Decoder::read_length_prefixed_borrowed` inherent
//! method directly.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode_view::<&[u8]>(data);
});
