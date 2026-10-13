//! Standard-library types added in 1.1 (audit finding PACK-028), and the
//! `encoded_len` contract for every built-in impl.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt::Debug;
use std::marker::PhantomData;
use std::num::{NonZeroI32, NonZeroU8, NonZeroU64, NonZeroUsize};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use pack_io::{Deserialize, SerialError, Serialize, decode, decode_view, encode};
use proptest::prelude::*;

fn round_trip<T: Serialize + Deserialize + PartialEq + Debug>(value: T) -> Vec<u8> {
    let bytes = encode(&value).unwrap();
    assert_eq!(value.encoded_len().unwrap(), bytes.len(), "{value:?}");
    assert_eq!(decode::<T>(&bytes).unwrap(), value);
    bytes
}

#[test]
fn test_char_is_a_scalar_value_varint() {
    assert_eq!(round_trip('a'), [0x61]);
    assert_eq!(round_trip('\u{e9}'), [0xe9, 0x01]);
    assert_eq!(round_trip('\u{10FFFF}'), encode(&0x10_FFFFu32).unwrap());
    assert_eq!(decode_view::<char>(&[0x61]).unwrap(), 'a');
    // Surrogate and out-of-range scalars are rejected.
    for bad in [0xD800u32, 0xDFFF, 0x11_0000, u32::MAX] {
        assert_eq!(
            decode::<char>(&encode(&bad).unwrap()),
            Err(SerialError::InvalidValue { kind: "char" })
        );
    }
}

#[test]
fn test_non_zero_integers_share_the_primitive_wire() {
    assert_eq!(round_trip(NonZeroU8::new(5).unwrap()), [5]);
    assert_eq!(
        round_trip(NonZeroU64::new(300).unwrap()),
        encode(&300u64).unwrap()
    );
    assert_eq!(
        round_trip(NonZeroI32::new(-2).unwrap()),
        encode(&-2i32).unwrap()
    );
    round_trip(NonZeroUsize::MAX);
    assert_eq!(
        decode::<NonZeroU64>(&[0]),
        Err(SerialError::InvalidValue { kind: "NonZeroU64" })
    );
    assert_eq!(
        decode::<NonZeroI32>(&[0]),
        Err(SerialError::InvalidValue { kind: "NonZeroI32" })
    );
}

#[test]
fn test_duration_is_seconds_then_nanos() {
    assert_eq!(round_trip(Duration::new(3, 5)), [3, 5]);
    round_trip(Duration::MAX);
    round_trip(Duration::ZERO);
    assert_eq!(
        decode_view::<Duration>(&[3, 5]).unwrap(),
        Duration::new(3, 5)
    );
    let too_many_nanos = encode(&(1u64, 1_000_000_000u32)).unwrap();
    assert_eq!(
        decode::<Duration>(&too_many_nanos),
        Err(SerialError::InvalidValue { kind: "Duration" })
    );
}

#[test]
fn test_transparent_wrappers_add_no_bytes() {
    let plain = encode(&String::from("txt")).unwrap();
    assert_eq!(round_trip(Box::<str>::from("txt")), plain);
    assert_eq!(round_trip(Rc::new(String::from("txt"))), plain);
    assert_eq!(round_trip(Arc::new(String::from("txt"))), plain);
    assert_eq!(round_trip(Rc::<str>::from("txt")), plain);
    assert_eq!(round_trip(Arc::<str>::from("txt")), plain);
    assert_eq!(round_trip(Cow::<str>::Owned(String::from("txt"))), plain);
    assert_eq!(encode(&Cow::Borrowed("txt")).unwrap(), plain);

    let seq = encode(&vec![1u32, 2]).unwrap();
    assert_eq!(round_trip(Box::<[u32]>::from(vec![1, 2])), seq);
    assert_eq!(round_trip(Rc::<[u32]>::from(vec![1, 2])), seq);
    assert_eq!(round_trip(Arc::<[u32]>::from(vec![1, 2])), seq);
    assert_eq!(round_trip(VecDeque::from(vec![1u32, 2])), seq);

    // A VecDeque whose storage wraps around still encodes front-to-back.
    let mut wrapped = VecDeque::with_capacity(4);
    wrapped.extend([0u32, 0, 1, 2]);
    let _ = wrapped.pop_front();
    let _ = wrapped.pop_front();
    wrapped.extend([3, 4]);
    assert_eq!(
        encode(&wrapped).unwrap(),
        encode(&vec![1u32, 2, 3, 4]).unwrap()
    );

    assert_eq!(round_trip(PhantomData::<String>), Vec::<u8>::new());
    assert_eq!(decode_view::<Box<str>>(&plain).unwrap(), Box::from("txt"));
}

#[test]
fn test_owning_cow_decode_yields_owned() {
    let bytes = encode(&"abc").unwrap();
    let cow: Cow<'static, str> = decode(&bytes).unwrap();
    assert!(matches!(cow, Cow::Owned(ref s) if s == "abc"));
}

proptest! {
    /// `encoded_len` agrees with `encode` for every built-in impl.
    #[test]
    fn prop_encoded_len_is_exact(
        a: u8, b: u16, c: u32, d: u64, e: u128, f: i8, g: i16, h: i32, i: i64, j: i128,
        k: f32, l: f64, m: bool, n: char, s: String,
        v: Vec<u32>, o: Option<i64>, bytes: Vec<u8>,
        map in proptest::collection::btree_map(any::<String>(), any::<i32>(), 0..8),
        hmap in proptest::collection::hash_map(any::<u16>(), any::<Vec<u8>>(), 0..8),
        set in proptest::collection::btree_set(any::<i64>(), 0..8),
        hset in proptest::collection::hash_set(any::<String>(), 0..8),
        secs: u64, nanos in 0u32..1_000_000_000,
    ) {
        let tuple = (a, b, c, d, e, f, g, h, i, j, k, l);
        prop_assert_eq!(tuple.encoded_len().unwrap(), encode(&tuple).unwrap().len());
        let more = (m, n, s.clone(), v.clone(), o, bytes.clone(), [a; 3], (), Ok::<_, String>(c));
        prop_assert_eq!(more.encoded_len().unwrap(), encode(&more).unwrap().len());
        prop_assert_eq!(map.encoded_len().unwrap(), encode(&map).unwrap().len());
        prop_assert_eq!(hmap.encoded_len().unwrap(), encode(&hmap).unwrap().len());
        prop_assert_eq!(set.encoded_len().unwrap(), encode(&set).unwrap().len());
        prop_assert_eq!(hset.encoded_len().unwrap(), encode(&hset).unwrap().len());
        let dur = Duration::new(secs, nanos);
        prop_assert_eq!(dur.encoded_len().unwrap(), encode(&dur).unwrap().len());
        let nested: BTreeMap<u8, Vec<Option<String>>> = BTreeMap::from([(a, vec![Some(s), None])]);
        prop_assert_eq!(nested.encoded_len().unwrap(), encode(&nested).unwrap().len());
    }

    /// The new types round-trip for every value.
    #[test]
    fn prop_new_types_round_trip(c: char, d in any::<(u64, u32)>(), nz in 1u64.., v: Vec<String>) {
        prop_assert_eq!(decode::<char>(&encode(&c).unwrap()).unwrap(), c);
        let dur = Duration::new(d.0, d.1 % 1_000_000_000);
        prop_assert_eq!(decode::<Duration>(&encode(&dur).unwrap()).unwrap(), dur);
        let nz = NonZeroU64::new(nz).unwrap();
        prop_assert_eq!(decode::<NonZeroU64>(&encode(&nz).unwrap()).unwrap(), nz);
        let deque: VecDeque<String> = v.iter().cloned().collect();
        prop_assert_eq!(decode::<VecDeque<String>>(&encode(&deque).unwrap()).unwrap(), deque);
        let boxed: Box<[String]> = v.into_boxed_slice();
        prop_assert_eq!(decode::<Box<[String]>>(&encode(&boxed).unwrap()).unwrap(), boxed);
    }

    /// Map / set encoding is independent of the source container and
    /// iteration order (the single-buffer encoder keeps the contract).
    #[test]
    fn prop_map_encoding_is_canonical(entries in proptest::collection::vec((any::<u32>(), any::<u8>()), 0..32)) {
        let b: BTreeMap<u32, u8> = entries.iter().copied().collect();
        let h: HashMap<u32, u8> = entries.iter().copied().collect();
        prop_assert_eq!(encode(&b).unwrap(), encode(&h).unwrap());
        let bs: BTreeSet<u32> = entries.iter().map(|e| e.0).collect();
        let hs: HashSet<u32> = entries.iter().map(|e| e.0).collect();
        prop_assert_eq!(encode(&bs).unwrap(), encode(&hs).unwrap());
    }
}
