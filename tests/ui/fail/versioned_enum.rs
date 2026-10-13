use pack_io::Serialize;

#[derive(Serialize)]
#[pack_io(version = 1)]
enum E {
    A,
}

fn main() {}
