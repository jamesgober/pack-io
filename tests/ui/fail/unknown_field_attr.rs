use pack_io::Serialize;

#[derive(Serialize)]
struct S {
    #[pack_io(rename = "b")]
    a: u32,
}

fn main() {}
