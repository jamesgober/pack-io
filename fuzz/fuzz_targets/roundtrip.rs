//! Fuzz target: differential round-trip checks.
//!
//! For every input that decodes, re-encoding and decoding again must give
//! the same value, `encoded_len` must match the encoding, and the encoder's
//! output must pass canonical mode. Inputs accepted in canonical mode must
//! be exactly the encoder's output (decode is injective, so bytes identify
//! values).

#![no_main]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Debug;

use libfuzzer_sys::fuzz_target;
use pack_io::{Config, Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Shape {
    Empty,
    Circle { r: u32 },
    Poly(Vec<(i16, i16)>),
    Named(String, Option<Box<str>>),
    Nested(Box<Shape>),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 2)]
struct Doc {
    title: String,
    shapes: Vec<Shape>,
    #[pack_io(since = 2)]
    index: BTreeMap<u32, BTreeSet<i64>>,
}

fn check<T>(data: &[u8])
where
    T: Serialize + Deserialize + PartialEq + Debug,
{
    if let Ok(value) = pack_io::decode::<T>(data) {
        let bytes = pack_io::encode(&value).expect("re-encode");
        assert_eq!(value.encoded_len().expect("encoded_len"), bytes.len());
        let again: T = pack_io::decode(&bytes).expect("decode own output");
        assert_eq!(again, value);
        let strict = Config::new().with_canonical(true);
        let canon: T = pack_io::decode_with_config(&bytes, strict).expect("canonical");
        assert_eq!(canon, value);
    }
}

fn check_canonical<T>(data: &[u8])
where
    T: Serialize + Deserialize + Debug,
{
    let strict = Config::new().with_canonical(true);
    if let Ok(value) = pack_io::decode_with_config::<T>(data, strict) {
        assert_eq!(pack_io::encode(&value).expect("re-encode"), data);
    }
}

fuzz_target!(|data: &[u8]| {
    check::<(u64, i32, String, Vec<u8>)>(data);
    check::<BTreeMap<u16, Vec<String>>>(data);
    check::<HashMap<u32, Option<char>>>(data);
    check::<Shape>(data);
    check::<Doc>(data);
    check_canonical::<(BTreeMap<u16, u32>, Vec<u64>, BTreeSet<i32>)>(data);
    check_canonical::<HashMap<u8, Option<u16>>>(data);
    check_canonical::<Shape>(data);
    check_canonical::<(String, u128, i128, u32)>(data);
    // Owning and borrowing decoders agree on borrowed shapes.
    let owned = pack_io::decode::<Vec<String>>(data);
    let view = pack_io::decode_view::<Vec<&str>>(data);
    assert_eq!(owned.is_ok(), view.is_ok());
    if let (Ok(o), Ok(v)) = (owned, view) {
        assert_eq!(o, v);
    }
});
