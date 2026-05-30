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
