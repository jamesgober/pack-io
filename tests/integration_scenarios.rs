//! Integration-shape tests — patterns real consumers (`network-protocol`,
//! `wire-codec`, Hive DB, raft log entries, message frames) actually write.
//!
//! These complement the unit tests and proptest harnesses: instead of
//! poking at individual primitives, each test here walks through a small
//! end-to-end scenario that mirrors how the substrate gets used in
//! production code. If the API has a usability gap that a real consumer
//! would hit, this is where it surfaces.

use std::io::{BufReader, BufWriter, Cursor};

use pack_io::{
    Decoder, Deserialize, DeserializeView, Encode, Encoder, IoDecoder, IoEncoder, Result,
    SerialError, Serialize, decode_from, decode_view, encode_into, peek_version,
};

// ===========================================================================
// Scenario 1: Length-framed message exchange — the shape every network
// protocol uses to delimit messages on a byte stream.
// ===========================================================================

/// A wire frame: `varint(payload_len) ++ payload_bytes`. Read on one side,
/// written on the other.
fn write_frame<W: std::io::Write, T: Serialize>(writer: &mut W, value: &T) -> std::io::Result<()> {
    let payload = pack_io::encode(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    // Length prefix using pack-io's own varint so the receiver can parse it
    // with `Decoder::read_varint_u64`.
    let mut len_buf = Encoder::with_capacity(10);
    Encode::write_varint_u64(&mut len_buf, payload.len() as u64).expect("infallible");
    writer.write_all(len_buf.as_bytes())?;
    writer.write_all(&payload)
}

fn read_frame<R: std::io::Read, T: Deserialize>(reader: &mut R) -> Result<T> {
    // Read the length prefix one byte at a time until the high bit clears.
    // (A real consumer would use a small intermediate buffer; this is the
    // straight-line version that mirrors the wire format spec.)
    let mut len: u64 = 0;
    let mut shift = 0u32;
    for byte_idx in 1..=10 {
        let mut b = [0u8; 1];
        reader
            .read_exact(&mut b)
            .map_err(|_| SerialError::UnexpectedEof {
                needed: 1,
                remaining: 0,
            })?;
        let byte = b[0];
        if byte_idx == 10 && (byte & 0xfe) != 0 {
            return Err(SerialError::VarintOverflow);
        }
        len |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    let len_usize = usize::try_from(len).map_err(|_| SerialError::IntegerOutOfRange)?;
    let mut payload = vec![0u8; len_usize];
    reader
        .read_exact(&mut payload)
        .map_err(|_| SerialError::UnexpectedEof {
            needed: len_usize,
            remaining: 0,
        })?;
    pack_io::decode(&payload)
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ChatMsg {
    user: String,
    body: String,
}

#[test]
fn length_framed_message_exchange_round_trips_three_messages() {
    let messages = vec![
        ChatMsg {
            user: "alice".into(),
            body: "hello, world".into(),
        },
        ChatMsg {
            user: "bob".into(),
            body: "ack".into(),
        },
        ChatMsg {
            user: "alice".into(),
            body: "ok bye".into(),
        },
    ];

    // Sender encodes every message into a single wire buffer.
    let mut wire: Vec<u8> = Vec::new();
    for msg in &messages {
        write_frame(&mut wire, msg).expect("write");
    }

    // Receiver pulls them back off, one frame at a time.
    let mut cursor = Cursor::new(wire);
    let mut received: Vec<ChatMsg> = Vec::new();
    while (cursor.position() as usize) < cursor.get_ref().len() {
        received.push(read_frame(&mut cursor).expect("read"));
    }

    assert_eq!(received, messages);
}

// ===========================================================================
// Scenario 2: Schema-versioned protocol negotiation — two services at
// different revisions of the same message type exchange payloads.
// ===========================================================================

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 1)]
struct HandshakeV1 {
    client_id: u64,
    capabilities: Vec<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 2)]
struct HandshakeV2 {
    client_id: u64,
    capabilities: Vec<String>,
    #[pack_io(since = 2)]
    region: Option<String>,
}

#[test]
fn versioned_handshake_round_trips_within_a_version() {
    let h = HandshakeV2 {
        client_id: 1001,
        capabilities: vec!["tls".into(), "h2".into(), "early-data".into()],
        region: Some("us-east-1".into()),
    };
    let bytes = pack_io::encode(&h).unwrap();
    let back: HandshakeV2 = pack_io::decode(&bytes).unwrap();
    assert_eq!(back, h);
}

#[test]
fn versioned_handshake_v1_client_decoded_by_v2_server_defaults_region() {
    let v1 = HandshakeV1 {
        client_id: 42,
        capabilities: vec!["tls".into()],
    };
    let bytes = pack_io::encode(&v1).unwrap();

    // The v2 server peeks the version, sees 1, and decodes as v2 — region
    // defaults to None because v1 never wrote it.
    assert_eq!(peek_version(&bytes).unwrap(), 1);
    let upgraded: HandshakeV2 = pack_io::decode(&bytes).unwrap();
    assert_eq!(upgraded.client_id, 42);
    assert_eq!(upgraded.capabilities, vec!["tls".to_string()]);
    assert_eq!(upgraded.region, None);
}

#[test]
fn versioned_handshake_v2_client_decoded_by_v1_server_drops_region() {
    let v2 = HandshakeV2 {
        client_id: 99,
        capabilities: vec!["h2".into(), "h3".into()],
        region: Some("eu-west-1".into()),
    };
    let bytes = pack_io::encode(&v2).unwrap();

    // The v1 server peeks the version, sees 2, decides whether to accept.
    // It can still decode the known fields — the trailing region bytes
    // are inside the length-framed body and get skipped.
    assert_eq!(peek_version(&bytes).unwrap(), 2);
    let downgraded: HandshakeV1 = pack_io::decode(&bytes).unwrap();
    assert_eq!(downgraded.client_id, 99);
    assert_eq!(
        downgraded.capabilities,
        vec!["h2".to_string(), "h3".to_string()]
    );
}

// ===========================================================================
// Scenario 3: Streaming multi-message event log — what every WAL / Kafka /
// raft log entry path looks like.
// ===========================================================================

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Event {
    Connected {
        peer: String,
        ts: u64,
    },
    Disconnected {
        peer: String,
        ts: u64,
        reason: String,
    },
    Message {
        from: String,
        to: String,
        body: Vec<u8>,
    },
    Heartbeat {
        ts: u64,
    },
}

#[test]
fn streaming_event_log_writes_and_reads_a_full_session() {
    let events = vec![
        Event::Connected {
            peer: "10.0.0.5".into(),
            ts: 1_000_000,
        },
        Event::Heartbeat { ts: 1_000_001 },
        Event::Message {
            from: "10.0.0.5".into(),
            to: "10.0.0.10".into(),
            body: vec![0xde, 0xad, 0xbe, 0xef],
        },
        Event::Heartbeat { ts: 1_000_002 },
        Event::Disconnected {
            peer: "10.0.0.5".into(),
            ts: 1_000_003,
            reason: "idle timeout".into(),
        },
    ];

    // Write the whole log through the streaming encoder.
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut writer = BufWriter::new(&mut buf);
        let mut enc = IoEncoder::new(&mut writer);
        for ev in &events {
            enc.write(ev).expect("write event");
        }
    }

    // Read it back, one event at a time, until the stream is exhausted.
    let reader = BufReader::new(Cursor::new(buf));
    let mut dec = IoDecoder::new(reader);
    let mut replayed: Vec<Event> = Vec::new();
    for _ in 0..events.len() {
        replayed.push(dec.read().expect("read event"));
    }
    assert_eq!(replayed, events);
}

// ===========================================================================
// Scenario 4: Zero-copy read path on the hot route — receiver borrows
// directly from the network buffer for read-only inspection.
// ===========================================================================

#[derive(Serialize)]
struct OwnedRequest {
    id: u64,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(DeserializeView)]
struct RequestView<'a> {
    id: u64,
    path: &'a str,
    #[allow(dead_code)]
    headers: Vec<(&'a str, &'a str)>,
    body: &'a [u8],
}

#[test]
fn zero_copy_request_inspection_avoids_per_field_allocation() {
    let owned = OwnedRequest {
        id: 7,
        path: "/api/v1/users/42".into(),
        headers: vec![
            ("host".into(), "example.com".into()),
            ("accept".into(), "application/json".into()),
            ("authorization".into(), "bearer …".into()),
        ],
        body: vec![0x7b, 0x7d], // "{}"
    };
    let wire = pack_io::encode(&owned).unwrap();

    let view: RequestView<'_> = decode_view(&wire).unwrap();

    // The view's `&str` / `&[u8]` fields must point inside `wire`, not into
    // freshly-allocated heap memory. The borrow-checker enforces it at
    // compile time; this asserts it at runtime as well, the way a security
    // audit would.
    let wire_start = wire.as_ptr() as usize;
    let wire_end = wire_start + wire.len();
    let path_ptr = view.path.as_ptr() as usize;
    let body_ptr = view.body.as_ptr() as usize;
    assert!((wire_start..wire_end).contains(&path_ptr));
    assert!((wire_start..wire_end).contains(&body_ptr));

    assert_eq!(view.id, 7);
    assert_eq!(view.path, "/api/v1/users/42");
    assert_eq!(view.body, b"{}");
}

// ===========================================================================
// Scenario 5: Configurable allocation cap protects a request-handler from
// hostile clients. The pattern every public-facing service should use.
// ===========================================================================

#[test]
fn tight_max_alloc_protects_decoder_from_oversized_payloads() {
    // Cap allocations at 16 KiB per length-prefixed value.
    let cfg = pack_io::Config::new().with_max_alloc(16 * 1024);

    // A hostile client declares a 100 MiB string. Decoder must reject
    // before allocating anything.
    let hostile_length_varint = {
        let mut enc = Encoder::new();
        Encode::write_varint_u64(&mut enc, 100 * 1024 * 1024).unwrap();
        enc.into_inner()
    };

    let mut dec = Decoder::with_config(&hostile_length_varint, cfg).unwrap();
    let err = dec
        .read::<String>()
        .expect_err("oversized payload rejected");
    match err {
        SerialError::InvalidLength { declared, .. } => {
            assert_eq!(declared, 100 * 1024 * 1024);
        }
        other => panic!("expected InvalidLength, got {other:?}"),
    }
}

// ===========================================================================
// Scenario 6: Single-shot helpers over arbitrary Read/Write — the patterns
// that wrap a TcpStream, a File, a Vec<u8>.
// ===========================================================================

#[test]
fn encode_into_decode_from_round_trip_through_cursor() {
    let original = ChatMsg {
        user: "alice".into(),
        body: "ping".into(),
    };

    // Encode straight into a byte buffer.
    let mut buf: Vec<u8> = Vec::new();
    encode_into(&original, &mut buf).expect("encode_into");

    // Decode straight back from the byte buffer wrapped in a Cursor.
    let back: ChatMsg = decode_from(&mut Cursor::new(buf)).expect("decode_from");

    assert_eq!(back, original);
}
