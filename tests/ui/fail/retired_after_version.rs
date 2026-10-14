use pack_io::Serialize;

// pack-io 2.0.0 (pre-release) accepted this, and `retired = 9` did nothing.
#[derive(Serialize)]
#[pack_io(version = 2)]
struct S {
    a: u8,
    #[pack_io(retired = 9)]
    b: u32,
}

fn main() {}
