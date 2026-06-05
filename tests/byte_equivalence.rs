//! Golden-vector tests — known input maps to known exact bytes.
//!
//! These tests run on every CI platform (Linux / macOS / Windows × stable
//! / MSRV). Passing on all six matrix cells *is* the cross-platform
//! byte-equivalence proof: if any platform produced different bytes for the
//! same input, the assertions here would fail there but not elsewhere.
//!
//! Treat each `assert_eq!` as the wire-format spec asserting itself. A
//! change to any of these byte sequences is a wire-format-breaking change
//! and requires the corresponding update to
//! [`docs/WIRE_FORMAT.md`](../docs/WIRE_FORMAT.md).

use std::collections::{BTreeMap, BTreeSet};

use pack_io::{decode, encode};

// ---------------------------------------------------------------------------
// Primitives — every integer type encodes a small value to a fixed sequence.
// ---------------------------------------------------------------------------

#[test]
fn u8_42_encodes_to_one_byte() {
    assert_eq!(encode(&42_u8).unwrap(), [0x2a]);
}

#[test]
fn u16_42_encodes_to_one_varint_byte() {
    assert_eq!(encode(&42_u16).unwrap(), [0x2a]);
}

#[test]
fn u32_42_encodes_to_one_varint_byte() {
    assert_eq!(encode(&42_u32).unwrap(), [0x2a]);
}

#[test]
fn u64_42_encodes_to_one_varint_byte() {
    assert_eq!(encode(&42_u64).unwrap(), [0x2a]);
}

#[test]
fn u64_128_encodes_to_two_varint_bytes() {
    // 128 is the first value that needs two varint bytes: 0x80 0x01.
    assert_eq!(encode(&128_u64).unwrap(), [0x80, 0x01]);
}

#[test]
fn u64_max_encodes_to_ten_varint_bytes() {
    let bytes = encode(&u64::MAX).unwrap();
    assert_eq!(bytes.len(), 10);
    assert_eq!(
        bytes,
        [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]
    );
}

#[test]
fn u128_max_encodes_to_nineteen_varint_bytes() {
    let bytes = encode(&u128::MAX).unwrap();
    assert_eq!(bytes.len(), 19);
}

#[test]
fn i8_neg_one_encodes_to_0xff() {
    // i8 uses raw two's-complement, not zigzag — single byte.
    assert_eq!(encode(&-1_i8).unwrap(), [0xff]);
}

#[test]
fn i64_neg_one_encodes_to_zigzag_one() {
    // ZigZag(−1) = 1, single varint byte.
    assert_eq!(encode(&-1_i64).unwrap(), [0x01]);
}

#[test]
fn i64_one_encodes_to_zigzag_two() {
    // ZigZag(1) = 2, single varint byte.
    assert_eq!(encode(&1_i64).unwrap(), [0x02]);
}

#[test]
fn i64_neg_two_encodes_to_zigzag_three() {
    // ZigZag(−2) = 3, single varint byte.
    assert_eq!(encode(&-2_i64).unwrap(), [0x03]);
}

#[test]
fn bool_false_encodes_to_zero_byte() {
    assert_eq!(encode(&false).unwrap(), [0x00]);
}

#[test]
fn bool_true_encodes_to_one_byte() {
    assert_eq!(encode(&true).unwrap(), [0x01]);
}

#[test]
fn unit_encodes_to_zero_bytes() {
    assert_eq!(encode(&()).unwrap(), []);
}

#[test]
fn f32_one_encodes_to_four_le_bytes() {
    // 1.0_f32 = 0x3F800000.
    assert_eq!(encode(&1.0_f32).unwrap(), [0x00, 0x00, 0x80, 0x3f]);
}

#[test]
fn f64_one_encodes_to_eight_le_bytes() {
    // 1.0_f64 = 0x3FF0000000000000.
    assert_eq!(
        encode(&1.0_f64).unwrap(),
        [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f]
    );
}

// ---------------------------------------------------------------------------
// Strings & byte slices
// ---------------------------------------------------------------------------

#[test]
fn empty_string_encodes_to_zero_length_prefix() {
    assert_eq!(encode(&String::new()).unwrap(), [0x00]);
}

#[test]
fn string_hello_encodes_to_known_bytes() {
    assert_eq!(
        encode(&"hello").unwrap(),
        [0x05, b'h', b'e', b'l', b'l', b'o']
    );
}

#[test]
fn vec_u8_known_payload_encodes_to_known_bytes() {
    let payload: Vec<u8> = vec![0x01, 0x02, 0x03, 0x04];
    assert_eq!(encode(&payload).unwrap(), [0x04, 0x01, 0x02, 0x03, 0x04]);
}

// ---------------------------------------------------------------------------
// Compound types
// ---------------------------------------------------------------------------

#[test]
fn option_none_encodes_to_zero_byte_only() {
    assert_eq!(encode(&None::<u64>).unwrap(), [0x00]);
}

#[test]
fn option_some_encodes_to_one_byte_tag_plus_value() {
    assert_eq!(encode(&Some(42_u64)).unwrap(), [0x01, 0x2a]);
}

#[test]
fn result_ok_encodes_to_zero_byte_tag_plus_value() {
    let value: Result<u64, String> = Ok(42);
    assert_eq!(encode(&value).unwrap(), [0x00, 0x2a]);
}

#[test]
fn result_err_encodes_to_one_byte_tag_plus_value() {
    let value: Result<u64, String> = Err("bad".into());
    assert_eq!(encode(&value).unwrap(), [0x01, 0x03, b'b', b'a', b'd']);
}

#[test]
fn tuple_encodes_to_concatenated_fields() {
    // (u8(7), bool(true), &str("hi")) = [0x07, 0x01, 0x02, b'h', b'i']
    let value = (7_u8, true, "hi");
    assert_eq!(encode(&value).unwrap(), [0x07, 0x01, 0x02, b'h', b'i']);
}

#[test]
fn array_encodes_to_concatenated_elements_no_length_prefix() {
    let value = [1_u32, 2, 3, 4];
    assert_eq!(encode(&value).unwrap(), [0x01, 0x02, 0x03, 0x04]);
}

// ---------------------------------------------------------------------------
// Map / set canonical ordering — same logical data, different concrete types
// must encode identically.
// ---------------------------------------------------------------------------

#[test]
fn btreemap_encodes_in_canonical_order() {
    // The spec sorts entries by their ENCODED key bytes, not by the
    // natural string order. For these keys, the length-prefix is the
    // first byte and dominates the comparison:
    //
    //   "alpha" encodes to [0x05, 'a', 'l', 'p', 'h', 'a']
    //   "beta"  encodes to [0x04, 'b', 'e', 't', 'a']
    //   "gamma" encodes to [0x05, 'g', 'a', 'm', 'm', 'a']
    //
    // Lexicographic byte comparison puts "beta" (length-prefix 0x04)
    // before "alpha" / "gamma" (length-prefix 0x05). Among the two
    // 5-byte-prefix entries, 'a' < 'g' so "alpha" sorts before "gamma".
    //
    // Canonical order: beta → alpha → gamma. This is the property that
    // makes a HashMap and a BTreeMap over the same data encode
    // identically. See `docs/WIRE_FORMAT.md §4.1`.
    let mut m: BTreeMap<&str, u32> = BTreeMap::new();
    let _ = m.insert("alpha", 1);
    let _ = m.insert("beta", 2);
    let _ = m.insert("gamma", 3);
    let bytes = encode(&m).unwrap();
    assert_eq!(
        bytes,
        [
            0x03, // count = 3
            0x04, b'b', b'e', b't', b'a', 0x02, // "beta" -> 2  (shortest prefix sorts first)
            0x05, b'a', b'l', b'p', b'h', b'a', 0x01, // "alpha" -> 1
            0x05, b'g', b'a', b'm', b'm', b'a', 0x03, // "gamma" -> 3
        ]
    );
}

#[cfg(feature = "std")]
#[test]
fn hashmap_and_btreemap_over_same_data_encode_identically() {
    use std::collections::HashMap;
    let mut h: HashMap<&str, u32> = HashMap::new();
    let _ = h.insert("zeta", 26);
    let _ = h.insert("alpha", 1);
    let _ = h.insert("mu", 13);

    let mut b: BTreeMap<&str, u32> = BTreeMap::new();
    let _ = b.insert("alpha", 1);
    let _ = b.insert("mu", 13);
    let _ = b.insert("zeta", 26);

    assert_eq!(encode(&h).unwrap(), encode(&b).unwrap());
}

#[test]
fn btreeset_encodes_in_canonical_order() {
    let mut s: BTreeSet<u32> = BTreeSet::new();
    let _ = s.insert(1);
    let _ = s.insert(7);
    let _ = s.insert(42);
    let bytes = encode(&s).unwrap();
    assert_eq!(bytes, [0x03, 0x01, 0x07, 0x2a]);
}

// ---------------------------------------------------------------------------
// Round-trip through every type — proves decode produces the same value the
// caller put in, on every platform.
// ---------------------------------------------------------------------------

#[test]
fn cross_platform_round_trip_struct() {
    // Same shape as the comparative benchmark and the release notes.
    let bytes = encode(&(42_u64, "hello".to_string(), vec![0xab_u8; 16], Some(true))).unwrap();

    // The decoded value must match exactly on every platform.
    let back: (u64, String, Vec<u8>, Option<bool>) = decode(&bytes).unwrap();
    assert_eq!(back.0, 42);
    assert_eq!(back.1, "hello");
    assert_eq!(back.2, vec![0xab; 16]);
    assert_eq!(back.3, Some(true));

    // And the bytes themselves must be the exact concatenation of the
    // per-field encodings.
    let expected: Vec<u8> = [0x2a] // u64(42)
        .iter()
        .chain([0x05, b'h', b'e', b'l', b'l', b'o'].iter()) // "hello"
        .chain([0x10].iter()) // Vec<u8> len = 16
        .chain([0xab; 16].iter()) // payload
        .chain([0x01, 0x01].iter()) // Some(true)
        .copied()
        .collect();
    assert_eq!(bytes, expected);
}
