// pack-io 1.0 rejected DeserializeView on types without a lifetime.
use pack_io::{DeserializeView, Serialize};

#[derive(Serialize, DeserializeView)]
struct Header {
    id: u64,
}

#[derive(Serialize, DeserializeView)]
struct Msg<'a> {
    h: Header,
    body: &'a str,
}

fn main() {
    let bytes = pack_io::encode(&Msg { h: Header { id: 1 }, body: "b" }).unwrap();
    let m: Msg<'_> = pack_io::decode_view(&bytes).unwrap();
    assert_eq!((m.h.id, m.body), (1, "b"));
}
