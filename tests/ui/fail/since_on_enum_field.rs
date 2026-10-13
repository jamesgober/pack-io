use pack_io::Serialize;

#[derive(Serialize)]
enum E {
    A {
        #[pack_io(since = 3)]
        y: u32,
    },
}

fn main() {}
