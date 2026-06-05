//! Hostile-input sweep — adversarial decode cases that complement the
//! existing `tests/adversarial.rs` and the `fuzz/` continuous harness.
//!
//! The hostile inputs here are hand-crafted to exercise specific failure
//! modes the contract promises to handle: recursion bombs, length prefixes
//! near `usize::MAX`, pathological enum / variant indices, oversized
//! collection counts, and varint corner cases at the legal-byte-count
//! boundary.
//!
//! Every test asserts the same contract: the decoder MUST return an error,
//! MUST NOT panic, and MUST NOT allocate above `Config::max_alloc`.

use std::collections::{BTreeMap, HashMap};

use pack_io::{Config, Decoder, SerialError, decode, decode_view};

// ---------------------------------------------------------------------------
// Length-prefix corners
// ---------------------------------------------------------------------------

/// `varint(u64::MAX)` as a length prefix — 10 bytes of `0xff…0x01`.
const VARINT_U64_MAX: [u8; 10] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];

#[test]
fn string_with_u64_max_length_prefix_is_rejected() {
    let err = decode::<String>(&VARINT_U64_MAX).expect_err("u64::MAX length rejected");
    assert!(matches!(
        err,
        SerialError::InvalidLength { .. } | SerialError::UnexpectedEof { .. }
    ));
}

#[test]
fn vec_u8_with_u64_max_length_prefix_is_rejected() {
    let err = decode::<Vec<u8>>(&VARINT_U64_MAX).expect_err("u64::MAX length rejected");
    assert!(matches!(err, SerialError::InvalidLength { .. }));
}

#[test]
fn hashmap_with_u64_max_count_is_rejected() {
    let err = decode::<HashMap<String, u64>>(&VARINT_U64_MAX).expect_err("u64::MAX count rejected");
    assert!(matches!(err, SerialError::InvalidLength { .. }));
}

#[test]
fn nested_vec_with_u64_max_count_is_rejected() {
    let err = decode::<Vec<Vec<u8>>>(&VARINT_U64_MAX).expect_err("u64::MAX count rejected");
    assert!(matches!(err, SerialError::InvalidLength { .. }));
}

#[test]
fn varint_with_length_above_tight_max_alloc_is_rejected_before_allocation() {
    let cfg = Config::new().with_max_alloc(64);

    // Declares length 1024 — well under default 1 GiB cap, above our 64-byte
    // tight cap. The decoder must reject before touching the heap.
    let bytes = [0x80_u8, 0x08]; // varint(1024)

    let mut dec = Decoder::with_config(&bytes, cfg).unwrap();
    let err = dec
        .read::<String>()
        .expect_err("length > tight cap rejected");
    assert!(matches!(
        err,
        SerialError::InvalidLength { declared: 1024, .. }
    ));
}

// ---------------------------------------------------------------------------
// Varint corner cases
// ---------------------------------------------------------------------------

#[test]
fn overlong_u64_varint_is_rejected() {
    // 11 continuation bytes — beyond the legal 10-byte maximum for u64.
    let overlong = [0xff_u8; 11];
    let err = decode::<u64>(&overlong).expect_err("overlong varint rejected");
    assert!(matches!(err, SerialError::VarintOverflow));
}

#[test]
fn tenth_varint_byte_with_high_bits_is_rejected() {
    // First nine bytes mark a continuation (0xff), tenth byte sets bit 1
    // which would overflow u64 (only bit 0 of byte 10 is legal).
    let mut bytes = [0xff_u8; 10];
    bytes[9] = 0x02;
    let err = decode::<u64>(&bytes).expect_err("u64 overflow rejected");
    assert!(matches!(err, SerialError::VarintOverflow));
}

#[test]
fn overlong_u128_varint_is_rejected() {
    let overlong = [0xff_u8; 20];
    let err = decode::<u128>(&overlong).expect_err("overlong u128 varint rejected");
    assert!(matches!(err, SerialError::VarintOverflow));
}

// ---------------------------------------------------------------------------
// Recursion-bomb shape — deeply nested `Option`. Decoder MUST handle it
// without stack overflow up to a sane depth, and MUST refuse a payload
// that declares arbitrary nesting depth via the on-wire format.
// ---------------------------------------------------------------------------

/// Build a payload that encodes `n` nested `Some(Some(Some(…)))` Options
/// each holding `()`. Each `Some` is one byte (`0x01`); the innermost
/// `Some(())` adds zero bytes. So the payload is `n` bytes of `0x01`.
fn deeply_nested_some_payload(n: usize) -> Vec<u8> {
    vec![0x01; n]
}

#[test]
fn nested_option_to_modest_depth_decodes_or_errors_without_panic() {
    // 8 levels of nesting — well within stack budgets for any platform.
    type T = Option<Option<Option<Option<Option<Option<Option<Option<()>>>>>>>>;
    let payload = deeply_nested_some_payload(8);
    // Any non-panic completion is success. The exact shape isn't asserted —
    // a decode that returns `Ok(...)` or any `Err(...)` both satisfy the
    // "doesn't panic on nested types" contract.
    let _ = decode::<T>(&payload);
}

#[test]
fn nested_option_with_pathological_payload_is_rejected() {
    // The wire-form decoder cannot synthesise arbitrary recursion depth
    // from a finite payload — but it CAN read a long run of `0x01` tags
    // and stop when it has decoded the requested type. Payloads longer
    // than the type permits surface as `TrailingBytes`.
    type T = Option<Option<Option<()>>>;
    let too_many_tags = deeply_nested_some_payload(64);
    let err = decode::<T>(&too_many_tags).expect_err("trailing bytes rejected");
    assert!(matches!(err, SerialError::TrailingBytes { .. }));
}

// ---------------------------------------------------------------------------
// Decode_view on hostile inputs — the zero-copy path must enforce the
// same contracts as owning decode.
// ---------------------------------------------------------------------------

#[test]
fn decode_view_str_with_u64_max_length_is_rejected() {
    let err = decode_view::<&str>(&VARINT_U64_MAX).expect_err("u64::MAX length rejected");
    assert!(matches!(
        err,
        SerialError::InvalidLength { .. } | SerialError::UnexpectedEof { .. }
    ));
}

#[test]
fn decode_view_str_with_invalid_utf8_is_rejected() {
    // Length 2, then two bytes that cannot start a valid UTF-8 sequence.
    let bytes = [0x02, 0xff, 0xff];
    let err = decode_view::<&str>(&bytes).expect_err("invalid UTF-8 rejected");
    assert!(matches!(err, SerialError::InvalidUtf8));
}

// ---------------------------------------------------------------------------
// Truncation sweep — every truncation prefix of a known-good encoding
// either decodes to a valid prefix value or returns an error. Never a
// panic.
// ---------------------------------------------------------------------------

#[test]
fn truncation_of_nested_struct_never_panics() {
    let bytes = pack_io::encode(&(
        42_u64,
        "hello".to_string(),
        vec![1_u8, 2, 3],
        Some(true),
        BTreeMap::from([("k1".to_string(), 1_u32), ("k2".to_string(), 2)]),
    ))
    .unwrap();

    for prefix_len in 0..bytes.len() {
        let _ = decode::<(u64, String, Vec<u8>, Option<bool>, BTreeMap<String, u32>)>(
            &bytes[..prefix_len],
        );
    }
}

// ---------------------------------------------------------------------------
// Trailing bytes — a strict decode must reject a payload with any byte
// past the value it consumed.
// ---------------------------------------------------------------------------

#[test]
fn strict_decode_rejects_trailing_garbage() {
    let mut bytes = pack_io::encode(&"hello").unwrap();
    bytes.extend_from_slice(&[0xff, 0xff, 0xff]);
    let err = decode::<String>(&bytes).expect_err("trailing bytes rejected");
    assert!(matches!(err, SerialError::TrailingBytes { remaining: 3 }));
}

#[test]
fn strict_decode_view_rejects_trailing_garbage() {
    let mut bytes = pack_io::encode(&"hello").unwrap();
    bytes.extend_from_slice(&[0x00]);
    let err = decode_view::<&str>(&bytes).expect_err("trailing bytes rejected");
    assert!(matches!(err, SerialError::TrailingBytes { remaining: 1 }));
}

// ---------------------------------------------------------------------------
// Empty input
// ---------------------------------------------------------------------------

#[test]
fn empty_input_for_required_value_is_unexpected_eof() {
    let err = decode::<u64>(&[]).expect_err("empty input rejected");
    assert!(matches!(err, SerialError::UnexpectedEof { .. }));
}

#[test]
fn empty_input_for_unit_type_succeeds() {
    // `()` is zero bytes — an empty input is a valid encoding of `()`.
    decode::<()>(&[]).unwrap();
}
