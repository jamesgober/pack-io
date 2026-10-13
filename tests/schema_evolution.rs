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
//! 5. Fields marked `#[pack_io(retired = N)]` keep their slot: encoders at
//!    version ≥ N write the field's default there, so readers of **every**
//!    version — including ones built before the retirement — stay aligned
//!    and never decode another field's bytes. Decoders at version ≥ N read
//!    and ignore the slot.
//! 6. Fields marked `#[pack_io(removed = N)]` (the 1.x `deprecated`
//!    encoding, kept so 1.x payloads still decode) are dropped by encoders
//!    at version ≥ N. Removing bytes from the middle of the body is only
//!    safe once every reader knows about it: the reader-first rollout (a
//!    reader at version N-1 that already declares `removed = N`) decodes
//!    version-N payloads correctly; a reader that predates the removal
//!    cannot, which `docs/WIRE_FORMAT.md` §3.8 spells out.
//! 7. Version 0 is reserved and rejected; body lengths are checked against
//!    `max_alloc` and the remaining input; the body is decoded under the
//!    caller's `Config`.

use pack_io::{
    Config, Deserialize, SerialError, Serialize, decode, decode_with_config, encode, peek_version,
};
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
    #[pack_io(removed = 3)]
    legacy_token: String, // gone as of v3
    #[pack_io(since = 2)]
    handle: String, // added in v2
    #[pack_io(since = 3)]
    bio: String, // added in v3
}

#[test]
fn removed_field_is_omitted_at_or_after_removal_version() {
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
    assert_eq!(v3.legacy_token, "token-xyz"); // read from wire (v1 < removed=3)
    assert_eq!(v3.handle, String::new()); // defaulted (since=2 > encoded=1)
    assert_eq!(v3.bio, String::new()); // defaulted (since=3 > encoded=1)
}

/// The reader-first rollout step for `AccountV3`: still writes version 2
/// (so `legacy_token` stays on the wire), but already declares that
/// version 3 removes it.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 2)]
struct AccountV2Announced {
    id: u64,
    #[pack_io(removed = 3)]
    legacy_token: String,
    #[pack_io(since = 2)]
    handle: String,
}

#[test]
fn removal_is_safe_after_a_reader_first_rollout() {
    // 1.0 shipped a test here asserting that a version-1 reader decodes a
    // version-3 payload with `legacy_token == "bob"` — the `handle` bytes
    // landing in the wrong field. That is silent corruption, not a
    // contract. With `removed`, the supported sequence is: first deploy
    // readers that declare `removed = 3`, then start writing version 3.
    // (`retired`, below, needs no such sequence.)
    let v3 = AccountV3 {
        id: 9,
        legacy_token: "ignored on encode".into(),
        handle: "bob".into(),
        bio: "world".into(),
    };
    let bytes = encode(&v3).unwrap();

    let reader: AccountV2Announced = decode(&bytes).unwrap();
    assert_eq!(
        reader,
        AccountV2Announced {
            id: 9,
            legacy_token: String::new(),
            handle: "bob".into(),
        }
    );

    // The announced reader still writes version 2, which every reader
    // (including a version-3 one) decodes with `legacy_token` intact.
    let v2_bytes = encode(&AccountV2Announced {
        id: 1,
        legacy_token: "tok".into(),
        handle: "h".into(),
    })
    .unwrap();
    let up: AccountV3 = decode(&v2_bytes).unwrap();
    assert_eq!(
        (
            up.legacy_token.as_str(),
            up.handle.as_str(),
            up.bio.as_str()
        ),
        ("tok", "h", "")
    );
    let v1: AccountV1 = decode(&v2_bytes).unwrap();
    assert_eq!(v1.legacy_token, "tok");
}

#[test]
fn removed_field_bytes_are_absent_from_the_wire() {
    // Pin the wire shape the rollout rule depends on: version 3 carries
    // [id, handle, bio] only.
    let bytes = encode(&AccountV3 {
        id: 1,
        legacy_token: "x".into(),
        handle: "h".into(),
        bio: "b".into(),
    })
    .unwrap();
    assert_eq!(bytes, [3, 5, 1, 1, b'h', 1, b'b']);
}

// ---------------------------------------------------------------------------
// Retired fields — the 2.0 fix for PACK-005: the slot stays, so no reader
// can ever decode another field's bytes into the retired one.
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 3)]
struct AccountV3Retired {
    id: u64,
    #[pack_io(retired = 3)]
    legacy_token: String,
    #[pack_io(since = 2)]
    handle: String,
    #[pack_io(since = 3)]
    bio: String,
}

/// The version-2 shape, as a reader built before the retirement sees it.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 2)]
struct AccountV2 {
    id: u64,
    legacy_token: String,
    #[pack_io(since = 2)]
    handle: String,
}

fn retired_v3() -> AccountV3Retired {
    AccountV3Retired {
        id: 9,
        legacy_token: "secret local value".into(),
        handle: "bob".into(),
        bio: "world".into(),
    }
}

#[test]
fn retired_field_keeps_its_slot_with_the_default_value() {
    let bytes = encode(&retired_v3()).unwrap();
    // [version 3][body 12][id 9][legacy "" ][handle "bob"][bio "world"]
    assert_eq!(
        bytes,
        [
            3, 12, 9, 0, 3, b'b', b'o', b'b', 5, b'w', b'o', b'r', b'l', b'd'
        ]
    );
    assert_eq!(retired_v3().encoded_len().unwrap(), bytes.len());
}

#[test]
fn readers_built_before_the_retirement_never_see_another_fields_bytes() {
    let bytes = encode(&retired_v3()).unwrap();
    // The 1.0 hazard: a version-1 reader got `legacy_token == "bob"`.
    let v1: AccountV1 = decode(&bytes).unwrap();
    assert_eq!((v1.id, v1.legacy_token.as_str()), (9, ""));
    let v2: AccountV2 = decode(&bytes).unwrap();
    assert_eq!(
        (v2.id, v2.legacy_token.as_str(), v2.handle.as_str()),
        (9, "", "bob")
    );
}

#[test]
fn retired_reader_handles_every_older_payload() {
    // Version-1 and version-2 payloads still carry a meaningful value.
    let v1 = encode(&AccountV1 {
        id: 1,
        legacy_token: "tok".into(),
    })
    .unwrap();
    let back: AccountV3Retired = decode(&v1).unwrap();
    assert_eq!(
        (back.legacy_token.as_str(), back.handle.as_str()),
        ("tok", "")
    );
    let v2 = encode(&AccountV2 {
        id: 2,
        legacy_token: "tok2".into(),
        handle: "h".into(),
    })
    .unwrap();
    let back: AccountV3Retired = decode(&v2).unwrap();
    assert_eq!(
        (back.legacy_token.as_str(), back.handle.as_str()),
        ("tok2", "h")
    );
}

#[test]
fn retired_slot_is_ignored_at_or_after_the_retirement_version() {
    // A version-3 payload whose slot holds a non-default value (written
    // by some other implementation) still decodes to the default.
    let bytes = [
        3, 13, 9, 1, b'x', 3, b'b', b'o', b'b', 5, b'w', b'o', b'r', b'l', b'd',
    ];
    let back: AccountV3Retired = decode(&bytes).unwrap();
    assert_eq!(back.legacy_token, "");
    assert_eq!(back.handle, "bob");
}

proptest! {
    /// Old readers decode retired-version payloads to exactly what they
    /// would get from an older writer that sent the default value.
    #[test]
    fn proptest_old_readers_agree_with_retired_writers(id: u64, token: String, handle: String, bio: String) {
        let bytes = encode(&AccountV3Retired { id, legacy_token: token, handle: handle.clone(), bio }).unwrap();
        let v2: AccountV2 = decode(&bytes).unwrap();
        prop_assert_eq!(v2, AccountV2 { id, legacy_token: String::new(), handle });
    }
}

// ---------------------------------------------------------------------------
// Frame validation (PACK-010, PACK-003)
// ---------------------------------------------------------------------------

#[test]
fn version_zero_is_rejected() {
    // 1.0 decoded this as `MessageV1 { id: 7, .. }` against the spec.
    let bytes = [0x00, 0x02, 0x07, 0x00];
    assert_eq!(
        decode::<MessageV1>(&bytes),
        Err(SerialError::InvalidVersion { version: 0 })
    );
}

#[test]
fn version_above_u32_is_rejected() {
    let mut bytes = encode(&(u64::from(u32::MAX) + 1)).unwrap();
    bytes.extend([0x00]);
    assert_eq!(
        decode::<MessageV1>(&bytes),
        Err(SerialError::IntegerOutOfRange)
    );
}

#[test]
fn body_length_beyond_the_input_is_rejected_before_reading() {
    let bytes = [0x01, 0x40, 0x07];
    assert!(matches!(
        decode::<MessageV1>(&bytes),
        Err(SerialError::InvalidLength {
            declared: 64,
            remaining: 1
        })
    ));
}

#[test]
fn body_is_decoded_under_the_callers_config() {
    let bytes = encode(&MessageV1 {
        id: 1,
        text: "x".repeat(100),
    })
    .unwrap();
    // The caller's cap applies to the body length and, inside the body, to
    // every field (see `tests/alloc_bounds.rs` for a body that is small
    // but declares an oversized inner field).
    let cfg = Config::new().with_max_alloc(110);
    assert!(decode_with_config::<MessageV1>(&bytes, cfg).is_ok());
    let cfg = Config::new().with_max_alloc(64);
    assert!(matches!(
        decode_with_config::<MessageV1>(&bytes, cfg),
        Err(SerialError::InvalidLength { .. })
    ));
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
