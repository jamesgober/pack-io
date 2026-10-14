//! Streaming codec tests: [`IoEncoder`] / [`IoDecoder`] round-trip equivalent
//! to the in-memory [`Encoder`] / [`Decoder`], and the convenience
//! [`encode_into`] / [`decode_from`] helpers.
//!
//! The invariant: the bytes a streaming encoder produces are **identical**
//! to the bytes the in-memory encoder produces for the same value. That is
//! the property that makes the two encoder flavours interchangeable.

use std::io::{Cursor, ErrorKind};

use pack_io::{IoDecoder, IoEncoder, SerialError, decode_from, encode, encode_into};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Equivalence with in-memory codec
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn streaming_encode_matches_in_memory_for_u64(v: u64) {
        let in_memory = encode(&v).unwrap();
        let mut streamed: Vec<u8> = Vec::new();
        encode_into(&v, &mut streamed).unwrap();
        prop_assert_eq!(in_memory, streamed);
    }

    #[test]
    fn streaming_encode_matches_in_memory_for_string(s: String) {
        let in_memory = encode(&s).unwrap();
        let mut streamed: Vec<u8> = Vec::new();
        encode_into(&s, &mut streamed).unwrap();
        prop_assert_eq!(in_memory, streamed);
    }

    #[test]
    fn streaming_encode_matches_in_memory_for_vec_u32(v: Vec<u32>) {
        let in_memory = encode(&v).unwrap();
        let mut streamed: Vec<u8> = Vec::new();
        encode_into(&v, &mut streamed).unwrap();
        prop_assert_eq!(in_memory, streamed);
    }
}

// ---------------------------------------------------------------------------
// Round-trip through Cursor
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn round_trip_string_through_cursor(s: String) {
        let mut buf: Vec<u8> = Vec::new();
        encode_into(&s, &mut buf).unwrap();
        let back: String = decode_from(&mut Cursor::new(buf)).unwrap();
        prop_assert_eq!(back, s);
    }

    #[test]
    fn round_trip_tuple_through_cursor(a: u64, b: String, c: bool) {
        let mut buf: Vec<u8> = Vec::new();
        encode_into(&(a, b.clone(), c), &mut buf).unwrap();
        let back: (u64, String, bool) = decode_from(&mut Cursor::new(buf)).unwrap();
        prop_assert_eq!(back, (a, b, c));
    }
}

// ---------------------------------------------------------------------------
// IoEncoder / IoDecoder direct use
// ---------------------------------------------------------------------------

#[test]
fn io_encoder_decoder_handle_many_values() {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut enc = IoEncoder::new(&mut buf);
        for n in 0u64..100 {
            enc.write(&n).unwrap();
        }
    }
    let mut dec = IoDecoder::new(Cursor::new(buf));
    for expected in 0u64..100 {
        let actual: u64 = dec.read().unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn io_encoder_into_inner_returns_underlying_writer() {
    let mut buf: Vec<u8> = Vec::new();
    let mut enc = IoEncoder::new(&mut buf);
    enc.write(&42_u64).unwrap();
    let writer = enc.into_inner();
    assert!(!writer.is_empty());
}

#[test]
fn io_decoder_into_inner_returns_underlying_reader() {
    let bytes = encode(&7u64).unwrap();
    let mut dec = IoDecoder::new(Cursor::new(bytes));
    let _: u64 = dec.read().unwrap();
    let cursor = dec.into_inner();
    assert_eq!(cursor.position() as usize, cursor.into_inner().len());
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

/// A `Read` impl that always fails with a specific kind, to exercise the
/// `std::io::Error` → `SerialError::Io` mapping path.
struct AlwaysFailReader;

impl std::io::Read for AlwaysFailReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            ErrorKind::PermissionDenied,
            "no read for you",
        ))
    }
}

#[test]
fn io_decoder_surfaces_permission_denied_through_serial_error() {
    let mut dec = IoDecoder::new(AlwaysFailReader);
    let err = dec
        .read::<u64>()
        .expect_err("permission denied should propagate");
    match err {
        SerialError::Io { kind, message } => {
            assert_eq!(kind, ErrorKind::PermissionDenied);
            assert!(message.contains("no read for you"));
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

/// A `Write` impl that always fails, to exercise the encode-side I/O error
/// mapping.
struct AlwaysFailWriter;

impl std::io::Write for AlwaysFailWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(ErrorKind::BrokenPipe, "pipe is broken"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn io_encoder_surfaces_broken_pipe_through_serial_error() {
    let mut enc = IoEncoder::new(AlwaysFailWriter);
    let err = enc
        .write(&42_u64)
        .expect_err("broken pipe should propagate");
    match err {
        SerialError::Io { kind, .. } => assert_eq!(kind, ErrorKind::BrokenPipe),
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn io_decoder_truncated_input_returns_unexpected_eof() {
    // varint continuation byte with nothing after it
    let bytes = vec![0x80u8];
    let mut dec = IoDecoder::new(Cursor::new(bytes));
    let err = dec.read::<u64>().expect_err("truncated input");
    assert!(matches!(err, SerialError::UnexpectedEof { .. }));
}

// ---------------------------------------------------------------------------
// Hostile and awkward readers
// ---------------------------------------------------------------------------

/// Hands out one byte per `read` call, and reports `Interrupted` before
/// every other byte. `read_exact` must retry through both.
struct Dribble {
    data: Vec<u8>,
    pos: usize,
    interrupt_next: bool,
}

impl std::io::Read for Dribble {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.interrupt_next {
            self.interrupt_next = false;
            return Err(std::io::Error::new(ErrorKind::Interrupted, "try again"));
        }
        self.interrupt_next = true;
        match (self.data.get(self.pos), buf.first_mut()) {
            (Some(&b), Some(slot)) => {
                *slot = b;
                self.pos += 1;
                Ok(1)
            }
            _ => Ok(0),
        }
    }
}

#[test]
fn io_decoder_survives_one_byte_reads_and_interrupts() {
    let value = (
        300_u32,
        String::from("dribbled"),
        vec![1u8; 70_000],
        Some(-5i64),
    );
    let mut dec = IoDecoder::new(Dribble {
        data: encode(&value).unwrap(),
        pos: 0,
        interrupt_next: true,
    });
    let back: (u32, String, Vec<u8>, Option<i64>) = dec.read().unwrap();
    assert_eq!(back, value);
}

#[test]
fn io_decoder_huge_declared_length_over_short_stream_is_eof() {
    // 512 MiB declared (under the default 1 GiB cap), 3 bytes delivered.
    let mut bytes = encode(&(1u64 << 29)).unwrap();
    bytes.extend_from_slice(b"abc");
    let mut dec = IoDecoder::new(Cursor::new(bytes));
    assert!(matches!(
        dec.read::<Vec<u8>>(),
        Err(SerialError::UnexpectedEof { .. })
    ));
}

#[test]
fn decode_from_is_capped_by_max_alloc() {
    use pack_io::{Config, decode_from_with_config};
    // Default cap (1 GiB) does not trip on ordinary payloads.
    let ok: Vec<u8> = decode_from(&mut Cursor::new(encode(&vec![1u8; 10_000]).unwrap())).unwrap();
    assert_eq!(ok.len(), 10_000);
    // A stream longer than the cap is cut off and rejected.
    let cfg = Config::new().with_max_alloc(100);
    let err =
        decode_from_with_config::<Vec<u8>, _>(&mut Cursor::new(vec![0u8; 1_000]), cfg).unwrap_err();
    assert!(matches!(
        err,
        SerialError::InvalidLength { declared: 101, .. }
    ));
    // Exactly at the cap is fine.
    let exact = encode(&vec![9u8; 98]).unwrap();
    assert_eq!(exact.len(), 99);
    let back: Vec<u8> = decode_from_with_config(&mut Cursor::new(exact), cfg).unwrap();
    assert_eq!(back.len(), 98);
}

#[test]
fn io_decoder_reports_its_config() {
    use pack_io::{Config, Decode};
    let cfg = Config::new().with_max_alloc(123).with_max_depth(9);
    let dec = IoDecoder::with_config(Cursor::new(Vec::new()), cfg).unwrap();
    assert_eq!(dec.config(), cfg);
    assert_eq!(Decode::config(&dec), cfg);
    assert_eq!(dec.remaining_hint(), None);
    assert!(dec.consumed().is_none());
}

/// PACK-R14: `IoEncoder` (and `BytesMut`) use the provided
/// `Encode::write_varint_u64` / `write_varint_u128`, while the in-memory
/// `Encoder` overrides both. Pin byte equality at every 7-bit boundary and
/// across the full `u128` / `i128` range.
#[test]
fn streaming_and_in_memory_encoders_agree_at_every_varint_boundary() {
    fn check<T: pack_io::Serialize + std::fmt::Debug>(value: T) {
        let memory = pack_io::encode(&value).unwrap();
        let mut streamed = Vec::new();
        IoEncoder::new(&mut streamed).write(&value).unwrap();
        assert_eq!(streamed, memory, "{value:?}");
        #[cfg(feature = "bytes")]
        {
            let mut buf = bytes::BytesMut::new();
            value.serialize(&mut buf).unwrap();
            assert_eq!(&buf[..], &memory[..], "{value:?} (BytesMut)");
        }
    }
    for shift in 0..64u32 {
        let edge = 1u64 << shift;
        for v in [edge - 1, edge, edge + 1] {
            check(v);
            check(v as i64);
            check((v as i64).wrapping_neg());
        }
    }
    check(u64::MAX);
    check(i64::MIN);
    for shift in 0..128u32 {
        let edge = 1u128 << shift;
        for v in [edge - 1, edge, edge.wrapping_add(1)] {
            check(v);
            check(v as i128);
        }
    }
    check(u128::MAX);
    check(i128::MIN);
    check(i128::MAX);
    // 128 is the first two-byte varint.
    let mut streamed = Vec::new();
    IoEncoder::new(&mut streamed).write(&128u64).unwrap();
    assert_eq!(streamed, [0x80, 0x01]);
    let mut streamed = Vec::new();
    IoEncoder::new(&mut streamed).write(&u128::MAX).unwrap();
    assert_eq!(streamed.len(), 19);
}
