use pack_io::Serialize;

#[derive(Serialize)]
#[pack_io(version = 3)]
struct S {
    #[pack_io(retired = 2, removed = 3)]
    a: u32,
}

fn main() {}
