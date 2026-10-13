use pack_io::Serialize;

// pack-io 2.0 split `deprecated` into `retired` and `removed`.
#[derive(Serialize)]
#[pack_io(version = 3)]
struct S {
    a: u32,
    #[pack_io(deprecated = 3)]
    b: u32,
}

fn main() {}
