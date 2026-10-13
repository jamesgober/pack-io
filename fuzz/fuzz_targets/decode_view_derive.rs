//! Fuzz target: arbitrary bytes → derive-generated `DeserializeView`.
//!
//! Covers the zero-copy derive on a versioned struct (whose frame the view
//! path ignored in 1.0) with `removed` and `retired` fields, an enum, a
//! lifetime-less nested type, and borrowed maps — and checks that the owning and borrowing decoders agree.

#![no_main]

use std::collections::BTreeMap;

use libfuzzer_sys::fuzz_target;
use pack_io::{Deserialize, DeserializeView, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize, DeserializeView)]
struct Id {
    shard: u16,
    seq: u64,
}

#[derive(Debug, Serialize, DeserializeView)]
#[pack_io(version = 3)]
struct ViewMsg<'a> {
    id: Id,
    text: &'a str,
    #[pack_io(removed = 3)]
    legacy: u32,
    #[pack_io(since = 2)]
    payload: &'a [u8],
    #[pack_io(since = 2, retired = 3)]
    note: &'a str,
    #[pack_io(since = 3)]
    tags: BTreeMap<&'a str, Option<Id>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[pack_io(version = 3)]
struct OwnedMsg {
    id: Id,
    text: String,
    #[pack_io(removed = 3)]
    legacy: u32,
    #[pack_io(since = 2)]
    payload: Vec<u8>,
    #[pack_io(since = 2, retired = 3)]
    note: String,
    #[pack_io(since = 3)]
    tags: BTreeMap<String, Option<Id>>,
}

#[derive(Debug, Serialize, DeserializeView)]
enum Frame<'a> {
    Ping,
    Data(&'a [u8], Id),
    Text { body: &'a str },
}

fuzz_target!(|data: &[u8]| {
    let view = pack_io::decode_view::<ViewMsg<'_>>(data);
    let owned = pack_io::decode::<OwnedMsg>(data);
    assert_eq!(view.is_ok(), owned.is_ok(), "view and owned decoders disagree");
    if let (Ok(v), Ok(o)) = (view, owned) {
        assert_eq!(v.id, o.id);
        assert_eq!(v.text, o.text);
        assert_eq!(v.legacy, o.legacy);
        assert_eq!(v.payload, &o.payload[..]);
        assert_eq!(v.note, o.note);
        assert_eq!(v.tags.len(), o.tags.len());
    }
    let _ = pack_io::decode_view::<Frame<'_>>(data);
});
