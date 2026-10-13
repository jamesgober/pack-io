use pack_io::Deserialize;

#[derive(Deserialize)]
struct S {
    #[pack_io(since = 2)]
    a: u32,
}

fn main() {}
