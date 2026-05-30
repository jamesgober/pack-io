//! Property-based determinism tests: encoding the same value twice produces
//! identical bytes.
//!
//! This is the safety contract for hashing, signing, and content-addressing.
//! If the same value can produce different bytes (e.g. via map iteration
//! order, padding bytes, or platform-dependent layout), then anything that
//! consumes the encoded bytes for identity is broken.
//!
//! For primitives, determinism is straightforward — there's no hidden
//! ordering or platform-dependent layout to test against. The point of
//! exercising it under `proptest` is to lock the property in place before
//! collections (`HashMap`, `HashSet`, …) land in `0.3`, where deterministic
//! key ordering becomes the load-bearing part of the implementation.

use pack_io::{Serialize, encode};
use proptest::prelude::*;

fn deterministic<T: Serialize>(value: &T) {
    let a = encode(value).expect("encode A");
    let b = encode(value).expect("encode B");
    assert_eq!(a, b);
}

proptest! {
    #[test]
    fn deterministic_u8(v: u8) { deterministic(&v); }

    #[test]
    fn deterministic_u16(v: u16) { deterministic(&v); }

    #[test]
    fn deterministic_u32(v: u32) { deterministic(&v); }

    #[test]
    fn deterministic_u64(v: u64) { deterministic(&v); }

    #[test]
    fn deterministic_u128(v: u128) { deterministic(&v); }

    #[test]
    fn deterministic_usize(v: usize) { deterministic(&v); }

    #[test]
    fn deterministic_i8(v: i8) { deterministic(&v); }

    #[test]
    fn deterministic_i16(v: i16) { deterministic(&v); }

    #[test]
    fn deterministic_i32(v: i32) { deterministic(&v); }

    #[test]
    fn deterministic_i64(v: i64) { deterministic(&v); }

    #[test]
    fn deterministic_i128(v: i128) { deterministic(&v); }

    #[test]
    fn deterministic_isize(v: isize) { deterministic(&v); }

    #[test]
    fn deterministic_bool(v: bool) { deterministic(&v); }

    #[test]
    fn deterministic_f32_bit_pattern(v: f32) { deterministic(&v); }

    #[test]
    fn deterministic_f64_bit_pattern(v: f64) { deterministic(&v); }

    #[test]
    fn deterministic_string(v: String) { deterministic(&v); }

    #[test]
    fn deterministic_vec_u8(v: Vec<u8>) { deterministic(&v); }

    #[test]
    fn deterministic_option_string(v: Option<String>) { deterministic(&v); }

    #[test]
    fn deterministic_tuple_three(a: u32, b: i64, c: String) {
        deterministic(&(a, b, c));
    }

    #[test]
    fn deterministic_array_eight_u32(arr in proptest::array::uniform8(any::<u32>())) {
        deterministic(&arr);
    }

    #[test]
    fn deterministic_nested(
        a in proptest::option::of(any::<String>()),
        b: Vec<u8>,
        c in proptest::array::uniform4(any::<u32>()),
    ) {
        deterministic(&(a, b, c));
    }
}
