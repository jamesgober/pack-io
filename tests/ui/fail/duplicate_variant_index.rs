use pack_io::Serialize;

#[derive(Serialize)]
enum E {
    #[pack_io(index = 1)]
    A,
    B,
    #[pack_io(index = 2)]
    C,
}

fn main() {}
