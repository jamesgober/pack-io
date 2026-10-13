use pack_io::Deserialize;

#[derive(Deserialize)]
struct S {
    #[pack_io(default = "make")]
    a: u32,
}

fn make() -> u32 {
    1
}

fn main() {}
