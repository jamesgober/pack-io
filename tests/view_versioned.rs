//! `DeserializeView` on versioned types (audit finding PACK-006) and the
//! zero-copy surface for every container.
//!
//! In 1.0 the view derive ignored `#[pack_io(version = N)]` and decoded the
//! version and body-length prefixes as if they were fields — a silent
//! misparse. The view decoder now reads the same frame the owning decoder
//! does, borrowing the body instead of copying it.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use pack_io::{
    Config, DeserializeView, SerialError, Serialize, decode, decode_view, decode_view_with_config,
    encode,
};

#[derive(Debug, PartialEq, Serialize, DeserializeView)]
#[pack_io(version = 2)]
struct VView<'a> {
    id: u64,
    name: &'a str,
}

#[test]
fn test_versioned_view_decodes_its_own_encoding() {
    // The audit reproduction: this used to decode to
    // `VView { id: 2, name: "\u{7}\u{3}abc" }`.
    let value = VView { id: 7, name: "abc" };
    let bytes = encode(&value).unwrap();
    assert_eq!(bytes, [2, 5, 7, 3, b'a', b'b', b'c']);
    let view: VView<'_> = decode_view(&bytes).unwrap();
    assert_eq!(view, value);
    // The string borrows from the input, inside the body.
    assert!(bytes.as_ptr_range().contains(&view.name.as_ptr()));
}

#[derive(Debug, PartialEq, Serialize, DeserializeView)]
#[pack_io(version = 1)]
struct ItemV1<'a> {
    sku: &'a str,
    legacy: u32,
}

/// The reader-first rollout step: still writes version 2 (with `legacy`),
/// but already knows that version 3 drops it.
#[derive(Debug, PartialEq, Serialize, DeserializeView)]
#[pack_io(version = 2)]
struct ItemV2<'a> {
    sku: &'a str,
    #[pack_io(removed = 3)]
    legacy: u32,
    #[pack_io(since = 2)]
    price: u64,
}

#[derive(Debug, PartialEq, Serialize, DeserializeView)]
#[pack_io(version = 3)]
struct ItemV3<'a> {
    sku: &'a str,
    #[pack_io(removed = 3)]
    legacy: u32,
    #[pack_io(since = 2)]
    price: u64,
    #[pack_io(since = 3)]
    tags: Vec<&'a str>,
}

#[test]
fn test_versioned_view_cross_version_rules_match_the_owning_decoder() {
    // Old payload, new reader: later fields default, the deprecated field
    // is still read (it is live at version 1).
    let v1 = encode(&ItemV1 {
        sku: "A-1",
        legacy: 5,
    })
    .unwrap();
    let new: ItemV3<'_> = decode_view(&v1).unwrap();
    assert_eq!(
        new,
        ItemV3 {
            sku: "A-1",
            legacy: 5,
            price: 0,
            tags: vec![]
        }
    );

    // New payload, reader that knows about the deprecation: `legacy` is
    // absent from the wire at version 3, `tags` is an unknown trailing
    // field and is skipped.
    let v3 = encode(&ItemV3 {
        sku: "B-2",
        legacy: 99,
        price: 1250,
        tags: vec!["sale", "new"],
    })
    .unwrap();
    let v2: ItemV2<'_> = decode_view(&v3).unwrap();
    assert_eq!(
        v2,
        ItemV2 {
            sku: "B-2",
            legacy: 0,
            price: 1250
        }
    );
    let same: ItemV3<'_> = decode_view(&v3).unwrap();
    assert_eq!(same.legacy, 0, "deprecated at 3, not on the wire");
    assert_eq!(
        (same.price, same.tags.as_slice()),
        (1250, &["sale", "new"][..])
    );

    // Version-2 payload, version-3 reader: `legacy` is still on the wire.
    let v2_bytes = encode(&ItemV2 {
        sku: "C-3",
        legacy: 7,
        price: 1,
    })
    .unwrap();
    let up: ItemV3<'_> = decode_view(&v2_bytes).unwrap();
    assert_eq!((up.legacy, up.price, up.tags.len()), (7, 1, 0));
}

#[test]
fn test_versioned_view_rejects_version_zero_and_bad_lengths() {
    assert_eq!(
        decode_view::<VView<'_>>(&[0, 1, 7]),
        Err(SerialError::InvalidVersion { version: 0 })
    );
    // Body length larger than the input.
    assert!(matches!(
        decode_view::<VView<'_>>(&[2, 50, 7]),
        Err(SerialError::InvalidLength { declared: 50, .. })
    ));
}

#[test]
fn test_versioned_view_body_obeys_the_callers_config() {
    let bytes = encode(&VView {
        id: 1,
        name: "a long enough name",
    })
    .unwrap();
    let tight = Config::new().with_max_alloc(8);
    assert!(matches!(
        decode_view_with_config::<VView<'_>>(&bytes, tight),
        Err(SerialError::InvalidLength { .. })
    ));
}

#[test]
fn test_owned_and_view_decoders_agree_on_versioned_payloads() {
    let bytes = encode(&VView { id: 3, name: "x" }).unwrap();
    #[derive(Debug, PartialEq, pack_io::Deserialize)]
    #[pack_io(version = 2)]
    struct Owned {
        id: u64,
        name: String,
    }
    let owned: Owned = decode(&bytes).unwrap();
    let view: VView<'_> = decode_view(&bytes).unwrap();
    assert_eq!((owned.id, owned.name.as_str()), (view.id, view.name));
}

// ---------------------------------------------------------------------------
// Container coverage for the view path
// ---------------------------------------------------------------------------

#[test]
fn test_view_containers_round_trip() {
    let set: BTreeSet<String> = ["b", "a"].iter().map(|s| (*s).to_owned()).collect();
    let bytes = encode(&set).unwrap();
    let view: BTreeSet<&str> = decode_view(&bytes).unwrap();
    assert_eq!(view, BTreeSet::from(["a", "b"]));

    let hset: HashSet<String> = ["x", "y", "z"].iter().map(|s| (*s).to_owned()).collect();
    let bytes = encode(&hset).unwrap();
    let view: HashSet<&str> = decode_view(&bytes).unwrap();
    assert_eq!(view.len(), 3);

    let map: HashMap<String, Vec<u8>> = HashMap::from([(String::from("k"), vec![1, 2])]);
    let bytes = encode(&map).unwrap();
    let view: HashMap<&str, &[u8]> = decode_view(&bytes).unwrap();
    assert_eq!(view["k"], &[1, 2]);

    let arr = [String::from("p"), String::from("q")];
    let bytes = encode(&arr).unwrap();
    let view: [&str; 2] = decode_view(&bytes).unwrap();
    assert_eq!(view, ["p", "q"]);

    let ok: Result<String, u8> = Ok(String::from("fine"));
    let err: Result<String, u8> = Err(4);
    assert_eq!(
        decode_view::<Result<&str, u8>>(&encode(&ok).unwrap()).unwrap(),
        Ok("fine")
    );
    assert_eq!(
        decode_view::<Result<&str, u8>>(&encode(&err).unwrap()).unwrap(),
        Err(4)
    );
    assert!(matches!(
        decode_view::<Result<&str, u8>>(&[7]),
        Err(SerialError::InvalidTag {
            kind: "Result",
            tag: 7
        })
    ));

    let deque: VecDeque<String> = VecDeque::from([String::from("d")]);
    let bytes = encode(&deque).unwrap();
    let view: VecDeque<&str> = decode_view(&bytes).unwrap();
    assert_eq!(view, VecDeque::from(["d"]));

    let cow_bytes = encode(&"borrowed").unwrap();
    let cow: Cow<'_, str> = decode_view(&cow_bytes).unwrap();
    assert!(matches!(cow, Cow::Borrowed("borrowed")));
    let blob = encode(&vec![9u8; 4]).unwrap();
    let cow: Cow<'_, [u8]> = decode_view(&blob).unwrap();
    assert!(matches!(cow, Cow::Borrowed(&[9, 9, 9, 9])));

    let owned_bytes: Vec<u8> = decode_view(&blob).unwrap();
    assert_eq!(owned_bytes, [9; 4]);
    let fixed: [u8; 3] = decode_view(&encode(&[1u8, 2, 3]).unwrap()).unwrap();
    assert_eq!(fixed, [1, 2, 3]);
}

#[test]
fn test_view_rejects_hostile_counts_before_allocating() {
    let hostile = [0xff, 0xff, 0xff, 0xff, 0x0f]; // varint(u32::MAX)
    assert!(decode_view::<Vec<&str>>(&hostile).is_err());
    assert!(decode_view::<Vec<u8>>(&hostile).is_err());
    assert!(decode_view::<HashMap<&str, u8>>(&hostile).is_err());
    assert!(decode_view::<BTreeMap<u8, &[u8]>>(&hostile).is_err());
    let tight = Config::new().with_max_alloc(16);
    assert!(matches!(
        decode_view_with_config::<Vec<&str>>(&[0x40], tight),
        Err(SerialError::InvalidLength { declared: 64, .. })
    ));
}

#[derive(Debug, PartialEq, Serialize, DeserializeView)]
#[pack_io(version = 2)]
struct Retiring<'a> {
    name: &'a str,
    #[pack_io(retired = 2)]
    old_note: &'a str,
    #[pack_io(since = 2)]
    count: u32,
}

#[test]
fn test_view_reads_and_ignores_retired_slots() {
    let bytes = encode(&Retiring {
        name: "n",
        old_note: "not written",
        count: 4,
    })
    .unwrap();
    // The slot is still there, holding the default (empty string).
    assert_eq!(bytes, [2, 4, 1, b'n', 0, 4]);
    let view: Retiring<'_> = decode_view(&bytes).unwrap();
    assert_eq!((view.name, view.old_note, view.count), ("n", "", 4));
    // A version-1 payload keeps its value.
    let v1 = [1, 4, 1, b'n', 1, b'x'];
    let view: Retiring<'_> = decode_view(&v1).unwrap();
    assert_eq!((view.old_note, view.count), ("x", 0));
}
