use pack_io::Serialize;

#[derive(Serialize)]
struct S {
    #[pack_io(skip)]
    #[pack_io(skip)]
    a: u32,
}

fn main() {}
