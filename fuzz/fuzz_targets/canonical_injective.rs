//! Fuzz target: canonical mode accepts one encoding per value.
//!
//! Whatever a canonical decoder accepts must re-encode to exactly the input
//! (re-audit finding PACK-R02, PACK-R03): shortest varints, sorted and
//! duplicate-free maps and sets, versioned structs only at the reader's own
//! version with no trailing body bytes and default-valued retired slots,
//! through the owning decoder, the view decoder and the serde adapter.

#![no_main]

use std::collections::{BTreeMap, BTreeSet};

use libfuzzer_sys::fuzz_target;
use pack_io::{Config, Deserialize, DeserializeView, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize, DeserializeView)]
#[pack_io(version = 3)]
struct Versioned {
    a: u8,
    #[pack_io(since = 2)]
    b: String,
    #[pack_io(retired = 3)]
    old: u32,
    map: BTreeMap<u16, Vec<u8>>,
    set: BTreeSet<i32>,
    inner: Option<Box<Versioned>>,
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct SerdeShape {
    a: u8,
    #[serde(with = "pack_io::serde::sorted_set")]
    set: BTreeSet<i32>,
    map: BTreeMap<u16, String>,
    list: Vec<i64>,
}

fuzz_target!(|data: &[u8]| {
    let strict = Config::new().with_canonical(true);
    if let Ok(v) = pack_io::decode_with_config::<Vec<Versioned>>(data, strict) {
        assert_eq!(pack_io::encode(&v).unwrap(), data, "owned");
    }
    if let Ok(v) = pack_io::decode_view_with_config::<Vec<Versioned>>(data, strict) {
        assert_eq!(pack_io::encode(&v).unwrap(), data, "view");
    }
    if let Ok(v) = pack_io::serde::from_slice_with_config::<SerdeShape>(data, strict) {
        assert_eq!(pack_io::serde::to_vec(&v).unwrap(), data, "serde");
    }
});
