// Generated code must only use `::core` and `::pack_io` paths, so it
// compiles in `#![no_std]` crates (pack-io 1.0 emitted `::std::vec!` for
// versioned structs).
#![no_std]
extern crate std as _std_for_the_test_harness;

use pack_io::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[pack_io(version = 2)]
struct Versioned {
    a: u32,
    #[pack_io(since = 2)]
    b: u8,
}

fn main() {}
