use pack_io::Serialize;

#[derive(Serialize)]
#[pack_io(version = 0)]
struct Msg {
    id: u64,
}

fn main() {}
