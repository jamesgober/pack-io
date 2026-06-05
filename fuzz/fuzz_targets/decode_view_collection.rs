//! Fuzz target: arbitrary bytes → `decode_view::<Vec<&str>>`.
//!
//! Zero-copy decode of a *collection* of borrowed strings. The container
//! itself allocates (the `Vec`), but each element is a borrow into the
//! input slice. Exercises the `DeserializeView` blanket impl on `Vec<T>`
//! plus the per-element view path in a single target.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode_view::<Vec<&str>>(data);
});
