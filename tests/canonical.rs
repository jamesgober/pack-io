//! Canonical decode mode (audit finding PACK-009, and PACK-R02 from the
//! 2.0 re-audit for versioned structs).
//!
//! The encoder has always produced one byte string per value. The decoder
//! is lenient by default (as the wire spec allows), which means several
//! byte strings decode to the same value. `Config::with_canonical(true)`
//! closes that gap: only the encoder's own output is accepted, so a hash
//! or signature over the bytes identifies the value.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Cursor;

use pack_io::{
    Config, Deserialize, DeserializeView, IoDecoder, SerialError, Serialize, decode,
    decode_view_with_config, decode_with_config, encode,
};
use proptest::prelude::*;

fn strict() -> Config {
    Config::new().with_canonical(true)
}

fn non_canonical(reason: &'static str) -> SerialError {
    SerialError::NonCanonical { reason }
}

// ---------------------------------------------------------------------------
// Varints
// ---------------------------------------------------------------------------

#[test]
fn test_overlong_varints_are_lenient_by_default_and_rejected_in_canonical_mode() {
    let cases: &[&[u8]] = &[
        &[0x80, 0x00],                   // 0 in two bytes
        &[0x81, 0x80, 0x80, 0x80, 0x00], // 1 in five bytes
        &[0xff, 0x80, 0x00],             // 127 in three bytes
    ];
    for bytes in cases {
        assert!(decode::<u64>(bytes).is_ok(), "{bytes:?} lenient");
        assert_eq!(
            decode_with_config::<u64>(bytes, strict()),
            Err(non_canonical("varint")),
            "{bytes:?} strict"
        );
    }
}

#[test]
fn test_overlong_varints_in_every_position_are_rejected() {
    // Length prefix of a string.
    let s = [0x82, 0x00, b'h', b'i'];
    assert_eq!(decode::<String>(&s).unwrap(), "hi");
    assert_eq!(
        decode_with_config::<String>(&s, strict()),
        Err(non_canonical("varint"))
    );
    // Signed integer (zigzag), u128, enum tag.
    assert_eq!(
        decode_with_config::<i32>(&[0x80, 0x00], strict()),
        Err(non_canonical("varint"))
    );
    assert_eq!(
        decode_with_config::<u128>(&[0x80, 0x80, 0x00], strict()),
        Err(non_canonical("varint"))
    );
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    enum E {
        A,
        B,
    }
    assert_eq!(decode::<E>(&[0x81, 0x00]).unwrap(), E::B);
    assert_eq!(
        decode_with_config::<E>(&[0x81, 0x00], strict()),
        Err(non_canonical("varint"))
    );
}

#[test]
fn test_shortest_varints_are_accepted_at_every_width_boundary() {
    for v in [0u64, 127, 128, 16_383, 16_384, u32::MAX as u64, u64::MAX] {
        let bytes = encode(&v).unwrap();
        assert_eq!(decode_with_config::<u64>(&bytes, strict()).unwrap(), v);
    }
    for v in [0u128, 1 << 64, u128::MAX] {
        let bytes = encode(&v).unwrap();
        assert_eq!(decode_with_config::<u128>(&bytes, strict()).unwrap(), v);
    }
}

// ---------------------------------------------------------------------------
// Map and set order
// ---------------------------------------------------------------------------

/// `[3, (2, 20), (1, 10), (2, 99)]`: unsorted and with a duplicate key.
const UNSORTED_DUP_MAP: [u8; 7] = [3, 2, 20, 1, 10, 2, 99];

#[test]
fn test_unsorted_and_duplicate_map_keys() {
    // Lenient: accepted (last write wins), and re-encoding differs.
    let lenient: BTreeMap<u32, u32> = decode(&UNSORTED_DUP_MAP).unwrap();
    assert_eq!(lenient, BTreeMap::from([(1, 10), (2, 99)]));
    assert_ne!(encode(&lenient).unwrap(), UNSORTED_DUP_MAP);

    for result in [
        decode_with_config::<BTreeMap<u32, u32>>(&UNSORTED_DUP_MAP, strict()).map(|_| ()),
        decode_with_config::<HashMap<u32, u32>>(&UNSORTED_DUP_MAP, strict()).map(|_| ()),
        decode_view_with_config::<BTreeMap<u32, u32>>(&UNSORTED_DUP_MAP, strict()).map(|_| ()),
        decode_view_with_config::<HashMap<u32, u32>>(&UNSORTED_DUP_MAP, strict()).map(|_| ()),
    ] {
        assert_eq!(result, Err(non_canonical("map key order")));
    }

    // Duplicate only (sorted otherwise).
    let dup = [2, 1, 10, 1, 11];
    assert_eq!(
        decode_with_config::<BTreeMap<u32, u32>>(&dup, strict()),
        Err(non_canonical("map key order"))
    );
}

#[test]
fn test_map_order_is_by_encoded_bytes_not_by_key_ord() {
    // 200 encodes as [0xc8, 0x01] and 300 as [0xac, 0x02]: by bytes 300
    // sorts first even though 200 < 300.
    let m: BTreeMap<u32, u8> = BTreeMap::from([(200, 1), (300, 2)]);
    let bytes = encode(&m).unwrap();
    assert_eq!(bytes, [2, 0xac, 0x02, 2, 0xc8, 0x01, 1]);
    assert_eq!(
        decode_with_config::<BTreeMap<u32, u8>>(&bytes, strict()).unwrap(),
        m
    );
    // The Ord-sorted order is therefore non-canonical.
    let ord_sorted = [2, 0xc8, 0x01, 1, 0xac, 0x02, 2];
    assert_eq!(
        decode_with_config::<BTreeMap<u32, u8>>(&ord_sorted, strict()),
        Err(non_canonical("map key order"))
    );
}

#[test]
fn test_unsorted_and_duplicate_set_elements() {
    let unsorted = [2, 5, 3];
    let dup = [2, 3, 3];
    assert!(decode::<BTreeSet<u8>>(&unsorted).is_ok());
    for bytes in [&unsorted[..], &dup[..]] {
        assert_eq!(
            decode_with_config::<BTreeSet<u8>>(bytes, strict()),
            Err(non_canonical("set element order"))
        );
        assert_eq!(
            decode_with_config::<HashSet<u8>>(bytes, strict()),
            Err(non_canonical("set element order"))
        );
        assert_eq!(
            decode_view_with_config::<BTreeSet<u8>>(bytes, strict()),
            Err(non_canonical("set element order"))
        );
    }
}

#[test]
fn test_nested_maps_are_checked_at_every_level() {
    let inner_bad = [1, 0, 2, 2, 0, 1, 0]; // {0: {2: 0, 1: 0}}
    assert!(decode::<BTreeMap<u8, BTreeMap<u8, u8>>>(&inner_bad).is_ok());
    assert_eq!(
        decode_with_config::<BTreeMap<u8, BTreeMap<u8, u8>>>(&inner_bad, strict()),
        Err(non_canonical("map key order"))
    );
}

// ---------------------------------------------------------------------------
// Versioned bodies
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize, DeserializeView)]
#[pack_io(version = 2)]
struct Msg {
    id: u32,
    #[pack_io(since = 2)]
    flag: bool,
}

#[test]
fn test_padding_inside_a_known_versioned_body_is_rejected_in_canonical_mode() {
    let good = encode(&Msg { id: 5, flag: true }).unwrap();
    assert_eq!(good, [2, 2, 5, 1]);
    // Same value, body padded with one extra byte.
    let padded = [2, 3, 5, 1, 0xee];
    assert_eq!(decode::<Msg>(&padded).unwrap(), Msg { id: 5, flag: true });
    assert_eq!(
        decode_with_config::<Msg>(&padded, strict()),
        Err(non_canonical("versioned body"))
    );
    assert_eq!(
        decode_view_with_config::<Msg>(&padded, strict()),
        Err(non_canonical("versioned body"))
    );
}

/// PACK-R02. A newer frame carries fields this reader cannot check (so any
/// bytes could be appended), and an older frame is a second encoding of
/// the defaulted value: canonical mode accepts only the reader's version.
/// Lenient mode keeps accepting both, which is the schema-evolution
/// contract every 1.x and 2.x reader follows.
#[test]
fn test_canonical_mode_accepts_only_the_readers_own_version() {
    let newer: [&[u8]; 3] = [
        &[3, 3, 5, 1, 0xee],
        &[9, 6, 5, 0, 0xde, 0xad, 0xbe, 0xef],
        &[9, 2, 5, 1],
    ];
    for bytes in newer {
        assert!(decode::<Msg>(bytes).is_ok(), "{bytes:?} lenient");
        assert_eq!(
            decode_with_config::<Msg>(bytes, strict()),
            Err(non_canonical("versioned struct version")),
            "{bytes:?}"
        );
        assert_eq!(
            decode_view_with_config::<Msg>(bytes, strict()),
            Err(non_canonical("versioned struct version")),
            "{bytes:?} view"
        );
    }

    // `[1, 1, 5]` is a version-1 frame for the value the encoder writes as
    // `[2, 2, 5, 0]`.
    let older = [1, 1, 5];
    assert_eq!(decode::<Msg>(&older).unwrap(), Msg { id: 5, flag: false });
    assert_eq!(encode(&Msg { id: 5, flag: false }).unwrap(), [2, 2, 5, 0]);
    assert_eq!(
        decode_with_config::<Msg>(&older, strict()),
        Err(non_canonical("versioned struct version"))
    );
    assert_eq!(
        decode_view_with_config::<Msg>(&older, strict()),
        Err(non_canonical("versioned struct version"))
    );
    assert_eq!(
        decode_with_config::<Msg>(&[2, 2, 5, 0], strict()).unwrap(),
        Msg { id: 5, flag: false }
    );
}

/// A version-3 struct whose `old` field retired at version 3: a conforming
/// writer always puts the default (`0`) in the slot.
#[derive(Debug, PartialEq, Serialize, Deserialize, DeserializeView)]
#[pack_io(version = 3)]
struct Retired {
    a: u8,
    #[pack_io(retired = 3)]
    old: u32,
    c: u8,
}

/// The same shape, with a retired field read through a `with` helper and
/// one whose type is a type parameter.
#[derive(Debug, PartialEq, Serialize, Deserialize, DeserializeView)]
#[pack_io(version = 2)]
struct RetiredGeneric<T: Default> {
    a: u8,
    #[pack_io(retired = 2)]
    t: T,
    #[pack_io(retired = 2, with = "as_string")]
    s: u16,
}

mod as_string {
    use pack_io::{Decode, Decoder, Deserialize, Encode, Result, Serialize};

    pub fn serialize<E: Encode + ?Sized>(v: &u16, e: &mut E) -> Result<()> {
        v.to_string().serialize(e)
    }

    pub fn deserialize<D: Decode + ?Sized>(d: &mut D) -> Result<u16> {
        String::deserialize(d)?
            .parse()
            .map_err(|_| pack_io::SerialError::custom("not a number"))
    }

    pub fn deserialize_view(d: &mut Decoder<'_>) -> Result<u16> {
        deserialize(d)
    }
}

#[test]
fn test_retired_slots_must_hold_the_default_in_canonical_mode() {
    let good = encode(&Retired {
        a: 1,
        old: 77,
        c: 3,
    })
    .unwrap();
    assert_eq!(good, [3, 3, 1, 0, 3]);
    let want = Retired { a: 1, old: 0, c: 3 };
    assert_eq!(
        decode_with_config::<Retired>(&good, strict()).unwrap(),
        want
    );
    assert_eq!(
        decode_view_with_config::<Retired>(&good, strict()).unwrap(),
        want
    );

    // No encoder writes 99 into the retired slot; lenient readers ignore it.
    let forged = [3, 3, 1, 99, 3];
    assert_eq!(decode::<Retired>(&forged).unwrap(), want);
    assert_eq!(
        decode_with_config::<Retired>(&forged, strict()),
        Err(non_canonical("retired field"))
    );
    assert_eq!(
        decode_view_with_config::<Retired>(&forged, strict()),
        Err(non_canonical("retired field"))
    );

    // Generic and `with` retired fields are checked against their own
    // default encodings (`0u64` and the string "0").
    let value = RetiredGeneric {
        a: 4,
        t: 9u64,
        s: 12,
    };
    let bytes = encode(&value).unwrap();
    assert_eq!(bytes, [2, 4, 4, 0, 1, b'0']);
    let back = RetiredGeneric {
        a: 4,
        t: 0u64,
        s: 0,
    };
    assert_eq!(
        decode_with_config::<RetiredGeneric<u64>>(&bytes, strict()).unwrap(),
        back
    );
    for forged in [[2, 4, 4, 1, 1, b'0'], [2, 4, 4, 0, 1, b'7']] {
        assert!(decode::<RetiredGeneric<u64>>(&forged).is_ok());
        assert_eq!(
            decode_with_config::<RetiredGeneric<u64>>(&forged, strict()),
            Err(non_canonical("retired field"))
        );
        assert_eq!(
            decode_view_with_config::<RetiredGeneric<u64>>(&forged, strict()),
            Err(non_canonical("retired field"))
        );
    }
}

// ---------------------------------------------------------------------------
// Decoders that cannot check canonical form refuse the mode up front.
// ---------------------------------------------------------------------------

#[test]
fn test_io_decoder_rejects_canonical_mode() {
    let err = IoDecoder::with_config(Cursor::new(vec![0u8]), strict()).unwrap_err();
    assert!(matches!(err, SerialError::InvalidConfig { .. }));
    // The supported route: buffer the frame, then decode it.
    let bytes = encode(&BTreeMap::from([(1u8, 2u8)])).unwrap();
    let back: BTreeMap<u8, u8> =
        pack_io::decode_from_with_config(&mut Cursor::new(bytes), strict()).unwrap();
    assert_eq!(back, BTreeMap::from([(1, 2)]));
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Record {
    id: u64,
    name: String,
    tags: BTreeSet<String>,
    scores: HashMap<u32, i64>,
    nested: Vec<BTreeMap<u16, Option<u128>>>,
}

fn record() -> impl Strategy<Value = Record> {
    (
        any::<u64>(),
        ".{0,12}",
        proptest::collection::btree_set(".{0,6}", 0..6),
        proptest::collection::hash_map(any::<u32>(), any::<i64>(), 0..8),
        proptest::collection::vec(
            proptest::collection::btree_map(any::<u16>(), any::<Option<u128>>(), 0..4),
            0..4,
        ),
    )
        .prop_map(|(id, name, tags, scores, nested)| Record {
            id,
            name,
            tags,
            scores,
            nested,
        })
}

proptest! {
    /// Every encoder output is canonical.
    #[test]
    fn prop_encoder_output_is_accepted_in_canonical_mode(r in record()) {
        let bytes = encode(&r).unwrap();
        prop_assert_eq!(decode_with_config::<Record>(&bytes, strict()).unwrap(), r);
    }

    /// Whatever canonical mode accepts re-encodes to exactly the same
    /// bytes: decode is injective, so bytes identify values.
    #[test]
    fn prop_canonical_mode_accepts_only_the_encoders_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..48)) {
        if let Ok(v) = decode_with_config::<(BTreeMap<u16, u32>, Vec<u64>, BTreeSet<i32>)>(&bytes, strict()) {
            prop_assert_eq!(encode(&v).unwrap(), bytes.clone());
        }
        if let Ok(v) = decode_with_config::<HashMap<u8, Option<u16>>>(&bytes, strict()) {
            prop_assert_eq!(encode(&v).unwrap(), bytes.clone());
        }
        // Versioned structs too (PACK-R02): canonical mode accepts only
        // frames at the reader's version, with no trailing body bytes and
        // retired slots holding their default.
        if let Ok(v) = decode_with_config::<Msg>(&bytes, strict()) {
            prop_assert_eq!(encode(&v).unwrap(), bytes.clone());
        }
        if let Ok(v) = decode_with_config::<Retired>(&bytes, strict()) {
            prop_assert_eq!(encode(&v).unwrap(), bytes.clone());
        }
        if let Ok(v) = decode_with_config::<Vec<Retired>>(&bytes, strict()) {
            prop_assert_eq!(encode(&v).unwrap(), bytes.clone());
        }
    }

    /// The same property over inputs that start as a valid versioned frame
    /// and are then mutated, which random bytes rarely reach.
    #[test]
    fn prop_canonical_versioned_frames_are_injective(
        a in any::<u8>(),
        c in any::<u8>(),
        edits in proptest::collection::vec((0usize..8, any::<u8>()), 0..4),
        tail in proptest::collection::vec(any::<u8>(), 0..3),
    ) {
        let mut bytes = encode(&Retired { a, old: 0, c }).unwrap();
        for (at, byte) in edits {
            if let Some(slot) = bytes.get_mut(at) {
                *slot = byte;
            }
        }
        bytes.extend(tail);
        if let Ok(v) = decode_with_config::<Retired>(&bytes, strict()) {
            prop_assert_eq!(encode(&v).unwrap(), bytes.clone());
        }
        if let Ok(v) = decode_view_with_config::<Retired>(&bytes, strict()) {
            prop_assert_eq!(encode(&v).unwrap(), bytes.clone());
        }
    }
}
