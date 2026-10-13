use pack_io::Serialize;

// pack-io 1.0 accepted this and silently never encoded `b`.
#[derive(Serialize)]
#[pack_io(version = 2)]
struct S {
    a: u32,
    #[pack_io(since = 9)]
    b: u32,
}

fn main() {}
