//! `bytes` feature: `Bytes` / `BytesMut` as values, and `BytesMut` as an
//! encode sink for tokio-style codecs.

use bytes::{Bytes, BytesMut};
use pack_io::{Config, SerialError, Serialize, decode, decode_view, decode_with_config, encode};

#[test]
fn test_bytes_share_the_vec_u8_wire_shape() {
    let payload = Bytes::from_static(b"hello");
    let as_bytes = encode(&payload).unwrap();
    assert_eq!(as_bytes, encode(&b"hello".to_vec()).unwrap());
    assert_eq!(as_bytes, encode(&BytesMut::from(&b"hello"[..])).unwrap());
    // Any of the three decodes any of the others.
    assert_eq!(decode::<Vec<u8>>(&as_bytes).unwrap(), b"hello");
    assert_eq!(decode::<Bytes>(&as_bytes).unwrap(), payload);
    assert_eq!(&decode::<BytesMut>(&as_bytes).unwrap()[..], b"hello");
    assert_eq!(decode_view::<&[u8]>(&as_bytes).unwrap(), b"hello");
}

#[test]
fn test_encoding_directly_into_bytes_mut() {
    #[derive(pack_io::Serialize)]
    struct Frame<'a> {
        id: u32,
        body: &'a str,
    }
    let mut buf = BytesMut::with_capacity(16);
    Frame { id: 1, body: "a" }.serialize(&mut buf).unwrap();
    Frame { id: 2, body: "bc" }.serialize(&mut buf).unwrap();
    assert_eq!(&buf[..], &[1, 1, b'a', 2, 2, b'b', b'c']);
}

#[test]
fn test_zero_copy_slices_of_a_frame() {
    let frame: Bytes = encode(&(42u16, vec![7u8; 1000])).unwrap().into();
    let (id, body): (u16, &[u8]) = decode_view(&frame).unwrap();
    let body = frame.slice_ref(body);
    assert_eq!((id, body.len()), (42, 1000));
    // `slice_ref` shares the frame's allocation instead of copying.
    assert!(frame.as_ptr_range().contains(&body.as_ptr()));
}

#[test]
fn test_bytes_respect_decoder_limits() {
    let big = encode(&Bytes::from(vec![0u8; 512])).unwrap();
    let tight = Config::new().with_max_alloc(64);
    assert!(matches!(
        decode_with_config::<Bytes>(&big, tight),
        Err(SerialError::InvalidLength { declared: 512, .. })
    ));
    assert!(matches!(
        decode_with_config::<BytesMut>(&big, tight),
        Err(SerialError::InvalidLength { declared: 512, .. })
    ));
    assert!(decode::<Bytes>(&big[..100]).is_err());
}
