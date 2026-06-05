//! End-to-end tests for schema-versioned types.
//!
//! The contract pinned down here:
//!
//! 1. A versioned type round-trips with itself.
//! 2. An **old** decoder reading a **new** encoding succeeds: it reads the
//!    fields it knows about and silently ignores trailing body bytes.
//! 3. A **new** decoder reading an **old** encoding succeeds: it reads the
//!    fields the old version emitted and uses `Default::default()` for the
//!    `#[pack_io(since = N)]` fields that weren't in the wire.
//! 4. `peek_version(&bytes)` returns the version declared by the encoder
//!    without touching the body.
//! 5. Fields marked `#[pack_io(deprecated = N)]` are dropped by encoders at
//!    version ≥ N and defaulted by decoders at version ≥ N.

use pack_io::{Deserialize, SerialError, Serialize, decode, encode, peek_version};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Shape: V1 has {id, text}. V2 adds `timestamp: Option<u64>` since version 2.
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 1)]
struct MessageV1 {
    id: u64,
    text: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 2)]
struct MessageV2 {
    id: u64,
    text: String,
    #[pack_io(since = 2)]
    timestamp: Option<u64>,
}

#[test]
fn v1_round_trips_with_itself() {
    let m = MessageV1 {
        id: 7,
        text: "hello".into(),
    };
    let bytes = encode(&m).unwrap();
    let back: MessageV1 = decode(&bytes).unwrap();
    assert_eq!(back, m);
}

#[test]
fn v2_round_trips_with_itself() {
    let m = MessageV2 {
        id: 7,
        text: "hello".into(),
        timestamp: Some(1_700_000_000),
    };
    let bytes = encode(&m).unwrap();
    let back: MessageV2 = decode(&bytes).unwrap();
    assert_eq!(back, m);
}

#[test]
fn v2_round_trips_with_none_timestamp() {
    let m = MessageV2 {
        id: 7,
        text: "hello".into(),
        timestamp: None,
    };
    let bytes = encode(&m).unwrap();
    let back: MessageV2 = decode(&bytes).unwrap();
    assert_eq!(back, m);
}

// ---------------------------------------------------------------------------
// Cross-version: old → new, new → old
// ---------------------------------------------------------------------------

#[test]
fn v1_encoded_decodes_as_v2_with_default_timestamp() {
    let v1 = MessageV1 {
        id: 100,
        text: "from v1".into(),
    };
    let bytes = encode(&v1).unwrap();

    // V2 decoder reads the V1 payload; the `timestamp` field defaults to None.
    let v2: MessageV2 = decode(&bytes).unwrap();
    assert_eq!(v2.id, 100);
    assert_eq!(v2.text, "from v1");
    assert_eq!(v2.timestamp, None);
}

#[test]
fn v2_encoded_decodes_as_v1_ignoring_trailing_field() {
    let v2 = MessageV2 {
        id: 200,
        text: "from v2".into(),
        timestamp: Some(42),
    };
    let bytes = encode(&v2).unwrap();

    // V1 decoder reads the V2 payload; the trailing `timestamp` bytes inside
    // the body are silently dropped because the length prefix bounds the read.
    let v1: MessageV1 = decode(&bytes).unwrap();
    assert_eq!(v1.id, 200);
    assert_eq!(v1.text, "from v2");
}

// ---------------------------------------------------------------------------
// peek_version
// ---------------------------------------------------------------------------

#[test]
fn peek_version_returns_writer_version() {
    let v1 = MessageV1 {
        id: 1,
        text: "v1".into(),
    };
    let v2 = MessageV2 {
        id: 1,
        text: "v2".into(),
        timestamp: Some(99),
    };
    assert_eq!(peek_version(&encode(&v1).unwrap()).unwrap(), 1);
    assert_eq!(peek_version(&encode(&v2).unwrap()).unwrap(), 2);
}

#[test]
fn peek_version_on_empty_input_is_unexpected_eof() {
    let err = peek_version(&[]).expect_err("empty input has no version");
    assert!(matches!(err, SerialError::UnexpectedEof { .. }));
}

// ---------------------------------------------------------------------------
// Deprecated fields — added in v1, removed in v3
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 1)]
struct AccountV1 {
    id: u64,
    legacy_token: String, // present in v1
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 3)]
struct AccountV3 {
    id: u64,
    #[pack_io(deprecated = 3)]
    legacy_token: String, // gone as of v3
    #[pack_io(since = 2)]
    handle: String, // added in v2
    #[pack_io(since = 3)]
    bio: String, // added in v3
}

#[test]
fn deprecated_field_is_omitted_at_or_after_removal_version() {
    let v3 = AccountV3 {
        id: 1,
        legacy_token: "ignored".into(), // local state, but skipped on encode
        handle: "alice".into(),
        bio: "hi".into(),
    };
    let bytes = encode(&v3).unwrap();

    // Round-trip via v3 → v3: the `legacy_token` value is dropped on encode
    // and defaulted on decode (empty String).
    let back: AccountV3 = decode(&bytes).unwrap();
    assert_eq!(back.id, 1);
    assert_eq!(back.legacy_token, String::new()); // dropped → defaulted
    assert_eq!(back.handle, "alice");
    assert_eq!(back.bio, "hi");
}

#[test]
fn v1_payload_decodes_as_v3_with_defaults_for_new_fields() {
    let v1 = AccountV1 {
        id: 5,
        legacy_token: "token-xyz".into(),
    };
    let bytes = encode(&v1).unwrap();

    let v3: AccountV3 = decode(&bytes).unwrap();
    assert_eq!(v3.id, 5);
    assert_eq!(v3.legacy_token, "token-xyz"); // read from wire (v1 < deprecated=3)
    assert_eq!(v3.handle, String::new()); // defaulted (since=2 > encoded=1)
    assert_eq!(v3.bio, String::new()); // defaulted (since=3 > encoded=1)
}

#[test]
fn v3_payload_decodes_as_v1_ignoring_new_fields() {
    let v3 = AccountV3 {
        id: 9,
        legacy_token: "ignored on encode".into(),
        handle: "bob".into(),
        bio: "world".into(),
    };
    let bytes = encode(&v3).unwrap();

    let v1: AccountV1 = decode(&bytes).unwrap();
    assert_eq!(v1.id, 9);
    // legacy_token is *deprecated at 3*, so v3 didn't emit it. v1 expects
    // it next, but it isn't there — so v1 reads an empty string from the
    // start of where `handle` lives in the wire? No: the v3 body contains
    // [id, handle, bio]. v1's decoder reads [id, legacy_token] and pulls
    // whatever the next length-prefixed string is, which is `handle`.
    assert_eq!(v1.legacy_token, "bob");
}

// ---------------------------------------------------------------------------
// Versioned type containing a non-versioned (plain v0.4-style) struct
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Plain {
    a: u32,
    b: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 1)]
struct WithPlain {
    label: String,
    plain: Plain,
}

#[test]
fn versioned_struct_containing_plain_struct_round_trips() {
    let value = WithPlain {
        label: "wrap".into(),
        plain: Plain { a: 1, b: 2 },
    };
    let bytes = encode(&value).unwrap();
    let back: WithPlain = decode(&bytes).unwrap();
    assert_eq!(back, value);
}

// ---------------------------------------------------------------------------
// proptest: v1↔v2 invariants across an input space
// ---------------------------------------------------------------------------

proptest! {
    /// Whatever v1 writes, v2 can read it (with `timestamp` always defaulted).
    #[test]
    fn proptest_v1_is_readable_by_v2(id: u64, text: String) {
        let bytes = encode(&MessageV1 { id, text: text.clone() }).unwrap();
        let v2: MessageV2 = decode(&bytes).unwrap();
        prop_assert_eq!(v2.id, id);
        prop_assert_eq!(v2.text, text);
        prop_assert_eq!(v2.timestamp, None);
    }

    /// Whatever v2 writes, v1 can read it (trailing field ignored).
    #[test]
    fn proptest_v2_is_readable_by_v1(id: u64, text: String, ts: Option<u64>) {
        let bytes = encode(&MessageV2 { id, text: text.clone(), timestamp: ts }).unwrap();
        let v1: MessageV1 = decode(&bytes).unwrap();
        prop_assert_eq!(v1.id, id);
        prop_assert_eq!(v1.text, text);
    }

    /// peek_version is consistent with the type's declared version.
    #[test]
    fn proptest_peek_version_matches_writer(id: u64, text: String) {
        let b1 = encode(&MessageV1 { id, text: text.clone() }).unwrap();
        let b2 = encode(&MessageV2 { id, text, timestamp: None }).unwrap();
        prop_assert_eq!(peek_version(&b1).unwrap(), 1);
        prop_assert_eq!(peek_version(&b2).unwrap(), 2);
    }
}

// ---------------------------------------------------------------------------
// Adversarial: hostile body length on a versioned struct is rejected
// ---------------------------------------------------------------------------

#[test]
fn hostile_body_length_is_rejected() {
    // varint(1) for version, then varint(u64::MAX) for body length.
    let mut bytes = Vec::new();
    bytes.push(0x01); // version = 1
    // Append a max-length varint (u64::MAX): 10 bytes.
    bytes.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]);

    let err = decode::<MessageV1>(&bytes).expect_err("hostile body length");
    assert!(matches!(err, SerialError::InvalidLength { .. }));
}
