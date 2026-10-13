// pack-io 1.0 failed to compile enums whose fields shadowed generated names.
use pack_io::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
enum E {
    A { __encoder: u32, __decoder: u32, __tag: u8, __other: u8 },
}

#[derive(Serialize, Deserialize)]
#[pack_io(version = 1)]
struct S {
    __body: u32,
    __version: u32,
    __body_len: u32,
    __len: u32,
}

fn main() {
    let e = E::A { __encoder: 1, __decoder: 2, __tag: 3, __other: 4 };
    let _ = pack_io::decode::<E>(&pack_io::encode(&e).unwrap()).unwrap();
    let s = S { __body: 1, __version: 2, __body_len: 3, __len: 4 };
    let _ = pack_io::decode::<S>(&pack_io::encode(&s).unwrap()).unwrap();
}
