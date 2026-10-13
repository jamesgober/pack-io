//! The `serde` adapter (audit finding PACK-012): 1.0 advertised it but the
//! feature compiled no code. These tests pin the contract that makes it
//! useful — serde-derived types produce the same bytes as the native
//! derive — plus the safety limits shared with the native decoder.

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;

use pack_io::serde::{
    Compat, from_decode, from_decoder, from_slice, from_slice_with_config, to_encoder, to_vec,
};
use pack_io::{Config, Decoder, Encoder, IoDecoder, SerialError, decode, encode};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// The same shapes, derived both ways.
// ---------------------------------------------------------------------------

mod native {
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
    pub struct Account {
        pub id: u64,
        pub handle: String,
        pub balance: i64,
        pub tags: Vec<String>,
        pub limits: BTreeMap<String, u32>,
        pub email: Option<String>,
        pub ratio: f64,
        pub flag: bool,
        pub code: char,
        pub big: u128,
        pub pair: (u8, i16),
        pub raw: Vec<u8>,
    }

    #[derive(Debug, Clone, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
    pub enum Event {
        Heartbeat,
        Login { user: u64, ip: String },
        Error(u32, String),
        Wrapped(Box<str>),
        Unit(()),
    }

    #[derive(Debug, Clone, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
    pub struct Newtype(pub u32);

    #[derive(Debug, Clone, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
    pub struct UnitStruct;
}

mod via_serde {
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct Account {
        pub id: u64,
        pub handle: String,
        pub balance: i64,
        pub tags: Vec<String>,
        pub limits: BTreeMap<String, u32>,
        pub email: Option<String>,
        pub ratio: f64,
        pub flag: bool,
        pub code: char,
        pub big: u128,
        pub pair: (u8, i16),
        #[serde(with = "serde_bytes_like")]
        pub raw: Vec<u8>,
    }

    /// serde serialises `Vec<u8>` as a sequence of `u8`, which already
    /// matches pack-io's `Vec<u8>` bytes; this module routes it through
    /// `serialize_bytes` instead to exercise that path too.
    mod serde_bytes_like {
        pub fn serialize<S: serde::Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
            s.serialize_bytes(v)
        }

        pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
            struct V;
            impl serde::de::Visitor<'_> for V {
                type Value = Vec<u8>;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("bytes")
                }
                fn visit_bytes<E>(self, v: &[u8]) -> Result<Vec<u8>, E> {
                    Ok(v.to_vec())
                }
                fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Vec<u8>, E> {
                    Ok(v)
                }
            }
            d.deserialize_byte_buf(V)
        }
    }

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    pub enum Event {
        Heartbeat,
        Login { user: u64, ip: String },
        Error(u32, String),
        Wrapped(Box<str>),
        Unit(()),
    }

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct Newtype(pub u32);

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct UnitStruct;
}

fn native_account() -> native::Account {
    native::Account {
        id: 1 << 40,
        handle: "alice".into(),
        balance: -12_345,
        tags: vec!["a".into(), "bb".into()],
        limits: BTreeMap::from([("z".into(), 1), ("a".into(), 300), ("m".into(), 70_000)]),
        email: Some("a@example.com".into()),
        ratio: 0.25,
        flag: true,
        code: '\u{1f980}',
        big: u128::MAX / 3,
        pair: (255, -2),
        raw: vec![0, 1, 2, 255],
    }
}

fn serde_account() -> via_serde::Account {
    let n = native_account();
    via_serde::Account {
        id: n.id,
        handle: n.handle,
        balance: n.balance,
        tags: n.tags,
        limits: n.limits,
        email: n.email,
        ratio: n.ratio,
        flag: n.flag,
        code: n.code,
        big: n.big,
        pair: n.pair,
        raw: n.raw,
    }
}

#[test]
fn test_serde_struct_bytes_equal_native_derive_bytes() {
    let native_bytes = encode(&native_account()).unwrap();
    let serde_bytes = to_vec(&serde_account()).unwrap();
    assert_eq!(serde_bytes, native_bytes);
    // And each side decodes the other's bytes.
    assert_eq!(
        from_slice::<via_serde::Account>(&native_bytes).unwrap(),
        serde_account()
    );
    assert_eq!(
        decode::<native::Account>(&serde_bytes).unwrap(),
        native_account()
    );
}

#[test]
fn test_serde_enum_bytes_equal_native_derive_bytes() {
    let pairs = [
        (native::Event::Heartbeat, via_serde::Event::Heartbeat),
        (
            native::Event::Login {
                user: 9,
                ip: "1.2.3.4".into(),
            },
            via_serde::Event::Login {
                user: 9,
                ip: "1.2.3.4".into(),
            },
        ),
        (
            native::Event::Error(500, "boom".into()),
            via_serde::Event::Error(500, "boom".into()),
        ),
        (
            native::Event::Wrapped("w".into()),
            via_serde::Event::Wrapped("w".into()),
        ),
        (native::Event::Unit(()), via_serde::Event::Unit(())),
    ];
    for (n, s) in pairs {
        let nb = encode(&n).unwrap();
        assert_eq!(to_vec(&s).unwrap(), nb, "{s:?}");
        assert_eq!(from_slice::<via_serde::Event>(&nb).unwrap(), s);
    }
    assert_eq!(
        to_vec(&via_serde::Newtype(300)).unwrap(),
        encode(&native::Newtype(300)).unwrap()
    );
    assert_eq!(
        to_vec(&via_serde::UnitStruct).unwrap(),
        encode(&native::UnitStruct).unwrap()
    );
    assert_eq!(
        from_slice::<via_serde::UnitStruct>(&[]).unwrap(),
        via_serde::UnitStruct
    );
}

#[test]
fn test_serde_hashmap_is_written_in_canonical_order() {
    let map: HashMap<u32, String> = (0..50u32).map(|i| (i * 7919, i.to_string())).collect();
    assert_eq!(to_vec(&map).unwrap(), encode(&map).unwrap());
    let back: HashMap<u32, String> = from_slice(&to_vec(&map).unwrap()).unwrap();
    assert_eq!(back, map);
}

#[test]
fn test_serde_borrows_strings_and_bytes_from_the_input() {
    #[derive(serde::Deserialize)]
    struct View<'a> {
        name: &'a str,
        #[serde(borrow)]
        blob: &'a [u8],
    }
    let bytes = encode(&("borrowed", b"raw".as_slice())).unwrap();
    let view: View<'_> = from_slice(&bytes).unwrap();
    assert_eq!((view.name, view.blob), ("borrowed", &b"raw"[..]));
    let range = bytes.as_ptr_range();
    assert!(range.contains(&view.name.as_ptr()));
    assert!(range.contains(&view.blob.as_ptr()));
}

#[test]
fn test_serde_sequences_of_unknown_length_are_buffered() {
    struct Unsized(Vec<u16>);
    impl serde::Serialize for Unsized {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeSeq;
            let mut seq = s.serialize_seq(None)?;
            for v in &self.0 {
                seq.serialize_element(v)?;
            }
            seq.end()
        }
    }
    let v = vec![1u16, 300, 7];
    assert_eq!(to_vec(&Unsized(v.clone())).unwrap(), encode(&v).unwrap());
}

#[test]
fn test_serde_sequence_length_lies_are_errors() {
    struct Liar;
    impl serde::Serialize for Liar {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeSeq;
            let mut seq = s.serialize_seq(Some(3))?;
            seq.serialize_element(&1u8)?;
            seq.end()
        }
    }
    assert!(matches!(to_vec(&Liar), Err(SerialError::Custom { .. })));
}

#[test]
fn test_serde_streams_through_encoder_and_io_decoder() {
    let mut enc = Encoder::new();
    to_encoder(&serde_account(), &mut enc).unwrap();
    to_encoder(&via_serde::Event::Heartbeat, &mut enc).unwrap();
    let bytes = enc.into_inner();

    let mut dec = Decoder::new(&bytes);
    let a: via_serde::Account = from_decoder(&mut dec).unwrap();
    let e: via_serde::Event = from_decoder(&mut dec).unwrap();
    assert!(dec.is_empty());
    assert_eq!((a, e), (serde_account(), via_serde::Event::Heartbeat));

    let mut io = IoDecoder::new(Cursor::new(bytes));
    let a: via_serde::Account = from_decode(&mut io).unwrap();
    assert_eq!(a, serde_account());
}

#[test]
fn test_serde_unsupported_features_fail_cleanly() {
    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    #[serde(untagged)]
    enum Untagged {
        A(u8),
        B(String),
    }
    let bytes = to_vec(&Untagged::A(1)).unwrap();
    assert!(matches!(
        from_slice::<Untagged>(&bytes),
        Err(SerialError::Custom { .. })
    ));
    assert!(matches!(
        from_slice::<serde::de::IgnoredAny>(&[0]),
        Err(SerialError::Custom { .. })
    ));
}

#[test]
fn test_serde_decode_limits_match_the_native_decoder() {
    // Depth.
    #[derive(serde::Deserialize, Debug)]
    #[allow(dead_code)]
    struct Node {
        kids: Vec<Node>,
    }
    let hostile = vec![1u8; 100_000];
    assert!(matches!(
        from_slice::<Node>(&hostile),
        Err(SerialError::DepthLimitExceeded { .. })
    ));
    // Allocation cap.
    let bytes = encode(&"x".repeat(100)).unwrap();
    let tight = Config::new().with_max_alloc(10);
    assert!(matches!(
        from_slice_with_config::<String>(&bytes, tight),
        Err(SerialError::InvalidLength { .. })
    ));
    // Canonical mode: unsorted map keys and overlong varints.
    let strict = Config::new().with_canonical(true);
    let unsorted = [2, 2, 0, 1, 0];
    assert!(from_slice::<BTreeMap<u8, u8>>(&unsorted).is_ok());
    assert_eq!(
        from_slice_with_config::<BTreeMap<u8, u8>>(&unsorted, strict),
        Err(SerialError::NonCanonical {
            reason: "map key order"
        })
    );
    assert_eq!(
        from_slice_with_config::<u16>(&[0x80, 0x00], strict),
        Err(SerialError::NonCanonical { reason: "varint" })
    );
    // Trailing bytes and bad tags.
    assert_eq!(
        from_slice::<u8>(&[1, 2]),
        Err(SerialError::TrailingBytes { remaining: 1 })
    );
    assert!(matches!(
        from_slice::<Option<u8>>(&[2]),
        Err(SerialError::InvalidTag { kind: "Option", .. })
    ));
    assert!(matches!(
        from_slice::<via_serde::Event>(&[42]),
        Err(SerialError::Custom { .. })
    ));
}

// ---------------------------------------------------------------------------
// Compat bridges
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize, pack_io::DeserializeView)]
#[pack_io(version = 1)]
struct Mixed {
    id: u64,
    #[pack_io(with = "pack_io::serde::compat")]
    at: Point,
    trail: Vec<Compat<Point>>,
}

#[test]
fn test_serde_types_inside_native_types() {
    let m = Mixed {
        id: 1,
        at: Point { x: -1, y: 2 },
        trail: vec![Compat(Point { x: 3, y: 4 })],
    };
    let bytes = encode(&m).unwrap();
    assert_eq!(decode::<Mixed>(&bytes).unwrap(), m);
    assert_eq!(pack_io::decode_view::<Mixed>(&bytes).unwrap(), m);
    assert_eq!(
        to_vec(&Point { x: -1, y: 2 }).unwrap(),
        encode(&Compat(Point { x: -1, y: 2 })).unwrap()
    );
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

type Wide = (
    u64,
    i32,
    String,
    Vec<u16>,
    Option<bool>,
    (u8, char),
    BTreeMap<String, i64>,
    HashMap<u32, u8>,
);

proptest! {
    #[test]
    fn prop_serde_and_native_bytes_agree(
        a: u64, b: i32, c: String, d: Vec<u16>, e: Option<bool>, f: (u8, char),
        g in proptest::collection::btree_map(any::<String>(), any::<i64>(), 0..6),
        h in proptest::collection::hash_map(any::<u32>(), any::<u8>(), 0..6),
    ) {
        let value = (a, b, c, d, e, f, g, h);
        let native = encode(&value).unwrap();
        prop_assert_eq!(to_vec(&value).unwrap(), native.clone());
        let back: Wide = from_slice(&native).unwrap();
        prop_assert_eq!(back, value);
    }

    #[test]
    fn prop_serde_decoder_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..96)) {
        let _ = from_slice::<via_serde::Account>(&bytes);
        let _ = from_slice::<via_serde::Event>(&bytes);
        let _ = from_slice::<(String, Vec<Option<u64>>, BTreeMap<u8, String>)>(&bytes);
        let mut io = IoDecoder::new(Cursor::new(&bytes[..]));
        let _ = from_decode::<Vec<String>, _>(&mut io);
    }
}
