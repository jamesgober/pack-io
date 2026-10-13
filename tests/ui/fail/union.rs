use pack_io::Serialize;

#[derive(Serialize)]
union Bits {
    a: u32,
    b: f32,
}

fn main() {}
