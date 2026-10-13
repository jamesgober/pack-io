use pack_io::Deserialize;

#[derive(Deserialize)]
enum E {
    #[pack_io(rename = "b")]
    B,
}

fn main() {}
