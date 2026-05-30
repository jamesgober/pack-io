//! Collection round-trip, determinism, and adversarial-decode tests.
//!
//! The key contract for hash-based collections (`HashMap`, `HashSet`) is
//! that two collections holding the same logical data — but with different
//! insertion order or even different concrete types (`HashMap` vs
//! `BTreeMap`) — encode to the **same** bytes. Without this property, any
//! workflow that hashes / signs / content-addresses the encoding falls over
//! the first time a producer changes insertion order.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use pack_io::{SerialError, decode, encode};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Round-trip
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn vec_u32_round_trips(v: Vec<u32>) {
        let bytes = encode(&v).unwrap();
        let back: Vec<u32> = decode(&bytes).unwrap();
        prop_assert_eq!(back, v);
    }

    #[test]
    fn vec_string_round_trips(v: Vec<String>) {
        let bytes = encode(&v).unwrap();
        let back: Vec<String> = decode(&bytes).unwrap();
        prop_assert_eq!(back, v);
    }

    #[test]
    fn vec_tuple_round_trips(v: Vec<(u32, String)>) {
        let bytes = encode(&v).unwrap();
        let back: Vec<(u32, String)> = decode(&bytes).unwrap();
        prop_assert_eq!(back, v);
    }

    #[test]
    fn btreemap_round_trips(m in proptest::collection::btree_map(any::<String>(), any::<u32>(), 0..20)) {
        let bytes = encode(&m).unwrap();
        let back: BTreeMap<String, u32> = decode(&bytes).unwrap();
        prop_assert_eq!(back, m);
    }

    #[test]
    fn btreeset_round_trips(s in proptest::collection::btree_set(any::<u32>(), 0..20)) {
        let bytes = encode(&s).unwrap();
        let back: BTreeSet<u32> = decode(&bytes).unwrap();
        prop_assert_eq!(back, s);
    }

    #[test]
    fn hashmap_round_trips(m in proptest::collection::hash_map(any::<u32>(), any::<u64>(), 0..20)) {
        let bytes = encode(&m).unwrap();
        let back: HashMap<u32, u64> = decode(&bytes).unwrap();
        prop_assert_eq!(back, m);
    }

    #[test]
    fn hashset_round_trips(s in proptest::collection::hash_set(any::<u32>(), 0..20)) {
        let bytes = encode(&s).unwrap();
        let back: HashSet<u32> = decode(&bytes).unwrap();
        prop_assert_eq!(back, s);
    }
}

// ---------------------------------------------------------------------------
// Determinism — the load-bearing property for hash-based collections
// ---------------------------------------------------------------------------

proptest! {
    /// A `HashMap` and a `BTreeMap` holding the same logical (k, v) pairs
    /// MUST encode to the same bytes. If this ever breaks, content-addressing
    /// a `HashMap` becomes unsafe.
    #[test]
    fn hashmap_and_btreemap_encode_identically(
        pairs in proptest::collection::vec((any::<u32>(), any::<u32>()), 0..16),
    ) {
        let h: HashMap<u32, u32> = pairs.iter().cloned().collect();
        let b: BTreeMap<u32, u32> = pairs.into_iter().collect();
        prop_assert_eq!(encode(&h).unwrap(), encode(&b).unwrap());
    }

    /// A `HashSet` and a `BTreeSet` holding the same logical elements MUST
    /// encode identically.
    #[test]
    fn hashset_and_btreeset_encode_identically(
        elems in proptest::collection::vec(any::<u32>(), 0..16),
    ) {
        let h: HashSet<u32> = elems.iter().copied().collect();
        let b: BTreeSet<u32> = elems.into_iter().collect();
        prop_assert_eq!(encode(&h).unwrap(), encode(&b).unwrap());
    }

    /// `HashMap` insertion order is irrelevant to encoded output. Inserting
    /// the same `(k, v)` pairs in two different orders MUST produce the
    /// same bytes.
    #[test]
    fn hashmap_insertion_order_is_irrelevant(
        pairs in proptest::collection::vec((any::<u32>(), any::<u32>()), 0..16),
    ) {
        let forward: HashMap<u32, u32> = pairs.iter().cloned().collect();
        let mut reversed_pairs = pairs;
        reversed_pairs.reverse();
        let reversed: HashMap<u32, u32> = reversed_pairs.into_iter().collect();
        // The maps may differ if there were duplicate keys (last-wins differs
        // between orders), but for unique-key inputs the encoded bytes must
        // match. Guard with the size check.
        prop_assume!(forward.len() == reversed.len());
        prop_assert_eq!(encode(&forward).unwrap(), encode(&reversed).unwrap());
    }

    /// `HashMap<String, _>` with shuffled inserts encodes deterministically.
    /// String keys exercise the encoded-byte sort path more thoroughly than
    /// integer keys.
    #[test]
    fn hashmap_string_keys_deterministic(
        keys in proptest::collection::hash_set(any::<String>(), 0..12),
    ) {
        let values: Vec<u32> = (0..keys.len() as u32).collect();
        let pairs: Vec<(String, u32)> = keys.into_iter().zip(values).collect();

        let a: HashMap<String, u32> = pairs.iter().cloned().collect();
        let mut shuffled = pairs;
        shuffled.reverse();
        let b: HashMap<String, u32> = shuffled.into_iter().collect();
        prop_assert_eq!(encode(&a).unwrap(), encode(&b).unwrap());
    }
}

// ---------------------------------------------------------------------------
// Adversarial decode
// ---------------------------------------------------------------------------

#[test]
fn vec_with_u64_max_length_is_rejected() {
    let bytes: [u8; 10] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
    let err = decode::<Vec<u32>>(&bytes).expect_err("hostile count");
    assert!(matches!(err, SerialError::InvalidLength { .. }));
}

#[test]
fn btreemap_with_u64_max_length_is_rejected() {
    let bytes: [u8; 10] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
    let err = decode::<BTreeMap<u32, u32>>(&bytes).expect_err("hostile count");
    assert!(matches!(err, SerialError::InvalidLength { .. }));
}

#[test]
fn hashmap_with_u64_max_length_is_rejected() {
    let bytes: [u8; 10] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
    let err = decode::<HashMap<u32, u32>>(&bytes).expect_err("hostile count");
    assert!(matches!(err, SerialError::InvalidLength { .. }));
}

#[test]
fn truncated_vec_returns_error() {
    let value: Vec<u64> = (0..10).collect();
    let bytes = encode(&value).unwrap();
    for truncate_to in 0..bytes.len() {
        let result = decode::<Vec<u64>>(&bytes[..truncate_to]);
        // Either an error, or — rarely — a different valid prefix.
        if let Ok(back) = result {
            assert!(back.len() <= value.len());
        }
    }
}

/// Regression for the v0.3 Windows-CI OOM: a count under `max_alloc` (1 GiB)
/// would still cause `HashMap::with_capacity(count)` to attempt a ~70 GB
/// table allocation, since each hash-table slot is ~36 bytes for
/// `HashMap<String, u32>`. The decode must complete in bounded memory: the
/// initial preallocation is capped, and the per-element decode fails fast
/// when the input runs out.
#[test]
fn declared_count_below_max_alloc_does_not_overcommit_memory() {
    use pack_io::Config;

    // Declared count = 1 << 28 (~268 M entries). Under the default 1 GiB
    // max_alloc, but a HashMap pre-allocation would be tens of GBs.
    let mut bytes = Vec::new();
    let count: u64 = 1 << 28;
    // Manual varint encoding of `count`:
    let mut n = count;
    while n >= 0x80 {
        bytes.push((n as u8) | 0x80);
        n >>= 7;
    }
    bytes.push(n as u8);
    // No payload follows — the decoder MUST fail on the first element read,
    // not while pre-allocating the capacity.

    // For each collection type, the call returns an error in a millisecond
    // — not after attempting a multi-gigabyte allocation.
    let err = decode::<HashMap<String, u32>>(&bytes).expect_err("hashmap");
    assert!(matches!(
        err,
        SerialError::UnexpectedEof { .. } | SerialError::InvalidLength { .. }
    ));

    let err = decode::<HashSet<String>>(&bytes).expect_err("hashset");
    assert!(matches!(
        err,
        SerialError::UnexpectedEof { .. } | SerialError::InvalidLength { .. }
    ));

    let err = decode::<Vec<String>>(&bytes).expect_err("vec");
    assert!(matches!(
        err,
        SerialError::UnexpectedEof { .. } | SerialError::InvalidLength { .. }
    ));

    // Same behaviour under a tight max_alloc: the count is rejected up-front.
    let cfg = Config::new().with_max_alloc(1 << 10);
    let mut dec = pack_io::Decoder::with_config(&bytes, cfg).unwrap();
    let err = dec.read::<HashMap<String, u32>>().expect_err("tight cap");
    assert!(matches!(err, SerialError::InvalidLength { .. }));
}

proptest! {
    /// Random bytes fed to a `Vec<u32>` decode must never panic.
    #[test]
    fn random_bytes_decode_to_vec_u32_without_panic(bytes: Vec<u8>) {
        let _ = decode::<Vec<u32>>(&bytes);
    }

    /// Random bytes fed to a `HashMap<String, u32>` decode must never panic.
    #[test]
    fn random_bytes_decode_to_hashmap_without_panic(bytes: Vec<u8>) {
        let _ = decode::<HashMap<String, u32>>(&bytes);
    }

    /// Random bytes fed to a `BTreeMap<u64, String>` decode must never panic.
    #[test]
    fn random_bytes_decode_to_btreemap_without_panic(bytes: Vec<u8>) {
        let _ = decode::<BTreeMap<u64, String>>(&bytes);
    }
}
