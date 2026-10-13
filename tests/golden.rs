//! Cross-version golden vectors.
//!
//! Every file under `tests/golden/` was written by **pack-io 1.0.1** from
//! the values below. The test proves, for each one, that the current
//! encoder still produces exactly those bytes and that the current decoder
//! reads them back to the same value. That is the 1.x wire-stability
//! promise checked against a real older release, not against literals
//! written in the same commit as the code under test.
//!
//! The files are never regenerated from the current tree. Setting
//! `PACK_IO_WRITE_GOLDEN=1` writes them, and is only meant to be used from
//! a checkout of the release that defines the baseline.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Debug;
use std::path::PathBuf;

use pack_io::{Deserialize, Serialize, decode, encode};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Account {
    id: u64,
    handle: String,
    flags: Vec<String>,
    active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Pair(i32, String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Unit;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Generic<T> {
    items: Vec<T>,
    head: Option<T>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Event {
    Heartbeat,
    Login { user: u64, ip: String },
    Error(u32, String),
    Batch(Vec<Event>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 1)]
struct MessageV1 {
    id: u64,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 2)]
struct MessageV2 {
    id: u64,
    text: String,
    #[pack_io(since = 2)]
    timestamp: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 3)]
struct AccountV3 {
    id: u64,
    #[pack_io(deprecated = 3)]
    legacy_token: String,
    #[pack_io(since = 2)]
    handle: String,
    #[pack_io(since = 3)]
    bio: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Envelope {
    seq: u32,
    inner: MessageV2,
    tail: Vec<MessageV1>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 4)]
struct Outer {
    envelope: Envelope,
    #[pack_io(since = 4)]
    tags: BTreeSet<String>,
}

/// Write or verify one vector.
fn vector<T>(name: &str, value: T)
where
    T: Serialize + Deserialize + PartialEq + Debug,
{
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "golden",
        &format!("{name}.bin"),
    ]
    .iter()
    .collect();
    let encoded = encode(&value).expect("encode");
    if std::env::var_os("PACK_IO_WRITE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, &encoded).expect("write golden");
        return;
    }
    let golden = std::fs::read(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(encoded, golden, "{name}: encoder output drifted from 1.0.1");
    let back: T = decode(&golden).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(
        back, value,
        "{name}: 1.0.1 bytes decode to a different value"
    );
}

#[test]
fn golden_primitives() {
    vector("u8", 200u8);
    vector("u16", 300u16);
    vector("u32", 70_000u32);
    vector("u64_max", u64::MAX);
    vector("u128", u128::MAX - 12_345);
    vector("i8", -5i8);
    vector("i16", -300i16);
    vector("i32_min", i32::MIN);
    vector("i64", -1i64);
    vector("i128_min", i128::MIN);
    vector("usize", 12_345usize);
    vector("isize", -12_345isize);
    vector("bool", true);
    vector("f32", 1.5f32);
    vector("f64", core::f64::consts::PI);
    vector("unit", ());
    vector("string", String::from("h\u{e9}llo w\u{f6}rld \u{1f980}"));
    vector("string_empty", String::new());
}

#[test]
fn golden_sequences() {
    vector("vec_u8", (0u8..=255).collect::<Vec<u8>>());
    vector("vec_u32", vec![0u32, 1, 127, 128, 16_384, u32::MAX]);
    vector(
        "vec_string",
        vec![String::from("a"), String::new(), String::from("ccc")],
    );
    vector("array_u8_32", [0xa5u8; 32]);
    vector("array_u16_3", [1u16, 300, 65_535]);
    vector("tuple", (7u64, String::from("x"), -3i32, false));
    vector("option_some", Some(String::from("hi")));
    vector("option_none", None::<u64>);
    vector("result_ok", Ok::<u32, String>(9));
    vector("result_err", Err::<u32, String>(String::from("nope")));
    vector(
        "nested",
        vec![
            Some((1u8, String::from("a"))),
            None,
            Some((2, String::new())),
        ],
    );
}

#[test]
fn golden_collections() {
    // Integer keys straddling the one/two/three-byte varint boundaries, so
    // the canonical (encoded-byte) order differs from numeric order.
    let ints: BTreeMap<u32, Vec<u8>> = [1u32, 127, 128, 200, 300, 16_384]
        .into_iter()
        .map(|k| (k, vec![k as u8; (k % 5) as usize]))
        .collect();
    vector("btreemap_u32", ints);
    let strs: BTreeMap<String, u32> = [("zeta", 5u32), ("alpha", 1), ("delta", 4), ("", 0)]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
    vector("btreemap_string", strs);
    vector(
        "btreeset_u64",
        [5u64, 500, 50_000, 0]
            .into_iter()
            .collect::<BTreeSet<u64>>(),
    );
    let hm: HashMap<u64, String> = (0..20u64).map(|i| (i * 977, format!("v{i}"))).collect();
    vector("hashmap", hm);
    let hs: HashSet<String> = ["b", "a", "c", "aa"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    vector("hashset", hs);
}

#[test]
fn golden_derived() {
    vector(
        "struct_plain",
        Account {
            id: 42,
            handle: String::from("alice"),
            flags: vec![String::from("admin"), String::from("beta")],
            active: true,
        },
    );
    vector("struct_tuple", Pair(-7, String::from("pair")));
    vector("struct_unit", Unit);
    vector(
        "struct_generic",
        Generic {
            items: vec![1u16, 2, 3],
            head: Some(9),
        },
    );
    vector("enum_unit", Event::Heartbeat);
    vector(
        "enum_named",
        Event::Login {
            user: 1_000_000,
            ip: String::from("10.0.0.1"),
        },
    );
    vector("enum_tuple", Event::Error(404, String::from("missing")));
    vector(
        "enum_nested",
        Event::Batch(vec![Event::Heartbeat, Event::Error(1, String::new())]),
    );
}

#[test]
fn golden_versioned() {
    vector(
        "versioned_v1",
        MessageV1 {
            id: 7,
            text: String::from("hello"),
        },
    );
    vector(
        "versioned_v2",
        MessageV2 {
            id: 7,
            text: String::from("hello"),
            timestamp: Some(1_700_000_000),
        },
    );
    vector(
        "versioned_v3_deprecated",
        AccountV3 {
            id: 9,
            // Not on the wire at version 3, so it decodes as the default.
            legacy_token: String::new(),
            handle: String::from("bob"),
            bio: String::from("world"),
        },
    );
    vector(
        "versioned_nested",
        Outer {
            envelope: Envelope {
                seq: 3,
                inner: MessageV2 {
                    id: 1,
                    text: String::from("in"),
                    timestamp: None,
                },
                tail: vec![MessageV1 {
                    id: 2,
                    text: String::from("t"),
                }],
            },
            tags: ["x", "y"].iter().map(|s| (*s).to_owned()).collect(),
        },
    );
}
