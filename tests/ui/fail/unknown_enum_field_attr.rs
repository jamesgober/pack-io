use pack_io::Serialize;

// pack-io 1.0 ignored attributes on enum fields and variants.
#[derive(Serialize)]
enum E {
    A {
        #[pack_io(skp)]
        x: u32,
    },
}

fn main() {}
