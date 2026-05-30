//! Property-based round-trip tests: `decode(encode(v)) == v` for every
//! supported primitive type.
//!
//! Round-trip integrity is the codec's defining contract. These tests sweep
//! the input space with `proptest` so we don't have to hand-write the cases
//! that are most likely to break (boundary values, large magnitudes,
//! negative ZigZag corners, …).
//!
//! Floats compare by bit pattern (`to_bits()`), not by `==`, because IEEE
//! 754 declares `NaN != NaN`. The wire format preserves float bit patterns
//! exactly; that is the property under test.

use pack_io::{Deserialize, Serialize, decode, encode};
use proptest::prelude::*;

fn round_trip_eq<T>(value: T)
where
    T: Serialize + Deserialize + PartialEq + core::fmt::Debug,
{
    let bytes = encode(&value).expect("encode is infallible for primitives");
    let back: T = decode(&bytes).expect("decode of a freshly-encoded value");
    assert_eq!(back, value);
}

proptest! {
    #[test]
    fn round_trip_u8(v: u8) { round_trip_eq(v); }

    #[test]
    fn round_trip_u16(v: u16) { round_trip_eq(v); }

    #[test]
    fn round_trip_u32(v: u32) { round_trip_eq(v); }

    #[test]
    fn round_trip_u64(v: u64) { round_trip_eq(v); }

    #[test]
    fn round_trip_u128(v: u128) { round_trip_eq(v); }

    #[test]
    fn round_trip_usize(v: usize) { round_trip_eq(v); }

    #[test]
    fn round_trip_i8(v: i8) { round_trip_eq(v); }

    #[test]
    fn round_trip_i16(v: i16) { round_trip_eq(v); }

    #[test]
    fn round_trip_i32(v: i32) { round_trip_eq(v); }

    #[test]
    fn round_trip_i64(v: i64) { round_trip_eq(v); }

    #[test]
    fn round_trip_i128(v: i128) { round_trip_eq(v); }

    #[test]
    fn round_trip_isize(v: isize) { round_trip_eq(v); }

    #[test]
    fn round_trip_bool(v: bool) { round_trip_eq(v); }

    #[test]
    fn round_trip_string(v: String) { round_trip_eq(v); }

    #[test]
    fn round_trip_vec_u8(v: Vec<u8>) { round_trip_eq(v); }

    #[test]
    fn round_trip_option_u64(v: Option<u64>) { round_trip_eq(v); }

    #[test]
    fn round_trip_option_string(v: Option<String>) { round_trip_eq(v); }

    #[test]
    fn round_trip_result_u64_string(v: Result<u64, String>) { round_trip_eq(v); }

    #[test]
    fn round_trip_tuple_three(a: u32, b: i64, c: String) {
        round_trip_eq((a, b, c));
    }

    #[test]
    fn round_trip_tuple_six(
        a: u8, b: u16, c: u32, d: u64, e: i32, f: bool,
    ) {
        round_trip_eq((a, b, c, d, e, f));
    }

    #[test]
    fn round_trip_array_four_u32(arr in proptest::array::uniform4(any::<u32>())) {
        round_trip_eq(arr);
    }

    #[test]
    fn round_trip_array_sixteen_u8(arr in proptest::array::uniform16(any::<u8>())) {
        round_trip_eq(arr);
    }

    #[test]
    fn round_trip_nested(
        a in proptest::option::of(any::<String>()),
        b: Vec<u8>,
        c in proptest::array::uniform3(any::<u32>()),
    ) {
        round_trip_eq((a, b, c));
    }
}

// Floats: compare by bit pattern. proptest's default `any::<f32>` /
// `any::<f64>` generates the full range including NaN/Inf/subnormals.

proptest! {
    #[test]
    fn round_trip_f32_bit_pattern(v: f32) {
        let bytes = encode(&v).expect("encode f32");
        let back: f32 = decode(&bytes).expect("decode f32");
        prop_assert_eq!(back.to_bits(), v.to_bits());
    }

    #[test]
    fn round_trip_f64_bit_pattern(v: f64) {
        let bytes = encode(&v).expect("encode f64");
        let back: f64 = decode(&bytes).expect("decode f64");
        prop_assert_eq!(back.to_bits(), v.to_bits());
    }
}
