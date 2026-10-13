//! Fuzz target: arbitrary bytes → the `serde` adapter.
//!
//! The serde path must hold the same guarantees as the native decoder (no
//! panic, bounded allocation and depth), and for shapes both derives can
//! express, both decoders must accept the same inputs and agree.

#![no_main]

use std::collections::BTreeMap;

use libfuzzer_sys::fuzz_target;

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct SerdeRecord {
    id: u64,
    name: String,
    tags: Vec<String>,
    attrs: BTreeMap<String, i64>,
    kind: SerdeKind,
    parent: Option<Box<SerdeRecord>>,
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
enum SerdeKind {
    A,
    B(u32),
    C { x: i16, y: i16 },
}

#[derive(Debug, PartialEq, serde::Deserialize)]
struct SerdeFlat {
    id: u64,
    name: String,
    tags: Vec<String>,
    attrs: BTreeMap<String, i64>,
    kind: SerdeKind,
}

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
struct NativeRecord {
    id: u64,
    name: String,
    tags: Vec<String>,
    attrs: BTreeMap<String, i64>,
    kind: NativeKind,
}

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
enum NativeKind {
    A,
    B(u32),
    C { x: i16, y: i16 },
}

fuzz_target!(|data: &[u8]| {
    if let Ok(rec) = pack_io::serde::from_slice::<SerdeRecord>(data) {
        let bytes = pack_io::serde::to_vec(&rec).expect("re-encode");
        let again: SerdeRecord = pack_io::serde::from_slice(&bytes).expect("decode own output");
        assert_eq!(again, rec);
    }
    let via_serde = pack_io::serde::from_slice::<SerdeFlat>(data);
    let native = pack_io::decode::<NativeRecord>(data);
    assert_eq!(via_serde.is_ok(), native.is_ok(), "serde and native disagree");
    if let (Ok(s), Ok(n)) = (via_serde, native) {
        assert_eq!(
            (s.id, &s.name, &s.tags, &s.attrs),
            (n.id, &n.name, &n.tags, &n.attrs)
        );
    }
    let _ = pack_io::serde::from_slice::<(String, Vec<Option<u64>>, BTreeMap<u8, String>)>(data);
});
