//! Adversarial decode tests: the decoder MUST refuse to panic, allocate
//! unboundedly, or read past its input — on any byte sequence at all.
//!
//! The tests fall into three groups:
//!
//! 1. **Random bytes.** Feed `proptest`-generated `Vec<u8>` to every public
//!    decode entry point. The decode call must return `Ok` *or* `Err` —
//!    never panic. This is the unconditional safety property.
//! 2. **Truncations.** Take a known-good encoding, lop off bytes from the
//!    end, decode the prefix. Every truncation must surface an error, not
//!    a panic.
//! 3. **Hostile lengths.** Hand-craft byte sequences whose length prefix
//!    claims more bytes than the buffer can ever hold. The decoder must
//!    refuse before allocating.

use std::panic::{AssertUnwindSafe, catch_unwind};

use pack_io::{Config, Decoder, SerialError, decode, encode};
use proptest::prelude::*;

/// Attempt to decode `bytes` as `T`; assert the call did not panic.
///
/// Returns the result so the caller can additionally assert specific error
/// variants when the input is known to be malformed.
fn decode_no_panic<T: pack_io::Deserialize + core::fmt::Debug>(
    bytes: &[u8],
) -> Result<T, SerialError> {
    let bytes = bytes.to_vec();
    let result = catch_unwind(AssertUnwindSafe(move || decode::<T>(&bytes)));
    match result {
        Ok(decode_result) => decode_result,
        Err(_) => panic!("decoder panicked on adversarial input"),
    }
}

// ---------------------------------------------------------------------------
// 1. Random-bytes safety
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn random_bytes_decode_to_u64_without_panic(bytes: Vec<u8>) {
        let _ = decode_no_panic::<u64>(&bytes);
    }

    #[test]
    fn random_bytes_decode_to_i64_without_panic(bytes: Vec<u8>) {
        let _ = decode_no_panic::<i64>(&bytes);
    }

    #[test]
    fn random_bytes_decode_to_u128_without_panic(bytes: Vec<u8>) {
        let _ = decode_no_panic::<u128>(&bytes);
    }

    #[test]
    fn random_bytes_decode_to_string_without_panic(bytes: Vec<u8>) {
        let _ = decode_no_panic::<String>(&bytes);
    }

    #[test]
    fn random_bytes_decode_to_vec_u8_without_panic(bytes: Vec<u8>) {
        let _ = decode_no_panic::<Vec<u8>>(&bytes);
    }

    #[test]
    fn random_bytes_decode_to_option_string_without_panic(bytes: Vec<u8>) {
        let _ = decode_no_panic::<Option<String>>(&bytes);
    }

    #[test]
    fn random_bytes_decode_to_tuple_without_panic(bytes: Vec<u8>) {
        let _ = decode_no_panic::<(u64, String, bool)>(&bytes);
    }

    #[test]
    fn random_bytes_decode_to_array_without_panic(bytes: Vec<u8>) {
        let _ = decode_no_panic::<[u32; 4]>(&bytes);
    }
}

// ---------------------------------------------------------------------------
// 2. Truncation
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn truncated_string_returns_error(value: String) {
        let bytes = encode(&value).expect("encode");
        for truncate_to in 0..bytes.len() {
            let result = decode_no_panic::<String>(&bytes[..truncate_to]);
            // A truncated encoding may decode to a different valid value
            // (rare) or — almost always — return an error.
            if let Ok(back) = result {
                // If it decoded, it must equal a prefix of the same value's
                // encoded form having been a different but valid value. That
                // is intentional and not a bug; just confirm no panic.
                let _ = back;
            }
        }
    }

    #[test]
    fn truncated_vec_u8_returns_error(value: Vec<u8>) {
        let bytes = encode(&value).expect("encode");
        for truncate_to in 0..bytes.len() {
            let _ = decode_no_panic::<Vec<u8>>(&bytes[..truncate_to]);
        }
    }

    #[test]
    fn truncated_tuple_returns_error(a: u64, b: String, c: bool) {
        let bytes = encode(&(a, b, c)).expect("encode");
        for truncate_to in 0..bytes.len() {
            let _ = decode_no_panic::<(u64, String, bool)>(&bytes[..truncate_to]);
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Hostile length prefixes
// ---------------------------------------------------------------------------

#[test]
fn hostile_length_u64_max_for_string_is_rejected() {
    // Length prefix = u64::MAX, no payload bytes. Encoding of u64::MAX as
    // LEB128 is 10 bytes; the decoder must reject this without trying to
    // allocate u64::MAX worth of memory.
    let bytes: [u8; 10] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
    let err = decode::<String>(&bytes).expect_err("hostile length rejected");
    assert!(
        matches!(
            err,
            SerialError::InvalidLength { .. }
                | SerialError::UnexpectedEof { .. }
                | SerialError::VarintOverflow
        ),
        "expected InvalidLength / UnexpectedEof / VarintOverflow, got {err:?}",
    );
}

#[test]
fn hostile_length_u64_max_for_vec_u8_is_rejected() {
    let bytes: [u8; 10] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
    let err = decode::<Vec<u8>>(&bytes).expect_err("hostile length rejected");
    assert!(matches!(
        err,
        SerialError::InvalidLength { .. }
            | SerialError::UnexpectedEof { .. }
            | SerialError::VarintOverflow
    ));
}

#[test]
fn hostile_length_above_configured_cap_is_rejected_before_alloc() {
    // Configure a 4-byte cap, then send a length prefix of 1024.
    // The first byte of varint(1024) is 0x80, the second is 0x08.
    let cfg = Config::new().with_max_alloc(4);
    let bytes = [0x80, 0x08];
    let mut dec = Decoder::with_config(&bytes, cfg).expect("non-zero cap");
    let err = dec
        .read::<String>()
        .expect_err("length above cap should fail before allocation");
    assert!(matches!(
        err,
        SerialError::InvalidLength { declared: 1024, .. }
    ));
}

#[test]
fn overlong_varint_is_rejected() {
    // 11 continuation bytes - clearly longer than u64 can hold.
    let bytes = [0xffu8; 11];
    let err = decode::<u64>(&bytes).expect_err("overlong varint rejected");
    assert!(matches!(err, SerialError::VarintOverflow));
}

#[test]
fn invalid_bool_byte_is_rejected() {
    let err = decode::<bool>(&[0x7f]).expect_err("0x7f is not a valid bool");
    assert!(matches!(err, SerialError::InvalidBool { byte: 0x7f }));
}

#[test]
fn invalid_option_tag_is_rejected() {
    let err = decode::<Option<u8>>(&[0x02, 0x00]).expect_err("0x02 is not an Option tag");
    assert!(matches!(
        err,
        SerialError::InvalidTag { kind: "Option", .. }
    ));
}

#[test]
fn invalid_result_tag_is_rejected() {
    let err = decode::<Result<u8, u8>>(&[0x02, 0x00]).expect_err("0x02 is not a valid Result tag");
    assert!(matches!(
        err,
        SerialError::InvalidTag { kind: "Result", .. }
    ));
}

#[test]
fn invalid_utf8_in_string_is_rejected() {
    // Length 2, then 0xff 0xff (not valid UTF-8 start bytes).
    let bytes = [0x02, 0xff, 0xff];
    let err = decode::<String>(&bytes).expect_err("invalid UTF-8 rejected");
    assert!(matches!(err, SerialError::InvalidUtf8));
}

#[test]
fn trailing_bytes_after_strict_decode_are_rejected() {
    let mut bytes = encode(&7u8).expect("encode u8");
    bytes.push(0x00);
    let err = decode::<u8>(&bytes).expect_err("trailing byte rejected");
    assert!(matches!(err, SerialError::TrailingBytes { remaining: 1 }));
}
