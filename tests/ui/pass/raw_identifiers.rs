// pack-io 1.0 panicked inside the derive: "__f_r#type" is not a valid identifier.
use pack_io::{Deserialize, DeserializeView, Serialize};

#[derive(Serialize, Deserialize, DeserializeView)]
#[pack_io(version = 1)]
struct R {
    r#type: u32,
    r#match: u8,
}

#[derive(Serialize, Deserialize)]
#[allow(non_camel_case_types)]
enum r#enum {
    r#struct { r#fn: u8 },
}

fn main() {
    let bytes = pack_io::encode(&R { r#type: 1, r#match: 2 }).unwrap();
    let back: R = pack_io::decode(&bytes).unwrap();
    assert_eq!((back.r#type, back.r#match), (1, 2));
    let _ = pack_io::encode(&r#enum::r#struct { r#fn: 3 }).unwrap();
}
