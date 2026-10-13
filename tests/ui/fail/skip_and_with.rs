use pack_io::Serialize;

#[derive(Serialize)]
struct S {
    #[pack_io(skip, with = "m")]
    a: u32,
}

fn main() {}
