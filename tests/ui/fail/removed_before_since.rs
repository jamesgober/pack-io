use pack_io::Serialize;

#[derive(Serialize)]
#[pack_io(version = 4)]
struct S {
    #[pack_io(since = 3, removed = 3)]
    a: u32,
}

fn main() {}
