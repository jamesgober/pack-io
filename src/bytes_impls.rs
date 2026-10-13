//! `bytes` crate integration (feature `bytes`).
//!
//! - [`bytes::Bytes`] and [`bytes::BytesMut`] implement [`Serialize`] /
//!   [`Deserialize`] with the same wire shape as `Vec<u8>` (a varint length
//!   prefix and the raw bytes), so they are interchangeable with `Vec<u8>`
//!   and `&[u8]` fields on either side of the wire.
//! - [`bytes::BytesMut`] implements [`Encode`], so a value can be encoded
//!   straight into a tokio codec's output buffer without an intermediate
//!   `Vec`.
//!
//! Zero-copy decoding from a `Bytes` frame needs no extra API: decode a
//! view with [`crate::decode_view`] and turn each borrowed `&[u8]` back into
//! a cheap, reference-counted slice of the frame with
//! [`Bytes::slice_ref`](bytes::Bytes::slice_ref):
//!
//! ```
//! use bytes::{Bytes, BytesMut};
//! use pack_io::{Serialize, decode_view};
//!
//! // Encode straight into a BytesMut, then freeze it into a frame.
//! let mut buf = BytesMut::new();
//! (7_u32, b"payload".as_slice()).serialize(&mut buf).unwrap();
//! let frame: Bytes = buf.freeze();
//!
//! let (id, body): (u32, &[u8]) = decode_view(&frame).unwrap();
//! let body: Bytes = frame.slice_ref(body); // shares the frame's allocation
//! assert_eq!((id, &body[..]), (7, &b"payload"[..]));
//! ```

use alloc::borrow::Cow;

use bytes::{BufMut, Bytes, BytesMut};

use crate::codec::{Decode, Encode, varint_len};
use crate::error::Result;
use crate::traits::{Deserialize, Serialize};

impl Serialize for Bytes {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        Serialize::serialize(self.as_ref(), encoder)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(self.len() as u64).saturating_add(self.len()))
    }
}

impl Deserialize for Bytes {
    /// Bounded exactly like `Vec<u8>`: the length is checked against
    /// `max_alloc` and the remaining input before anything is allocated.
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        decoder.read_length_prefixed().map(Bytes::from)
    }
}

impl Serialize for BytesMut {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        Serialize::serialize(self.as_ref(), encoder)
    }

    #[inline]
    fn encoded_len(&self) -> Result<usize> {
        Ok(varint_len(self.len() as u64).saturating_add(self.len()))
    }
}

impl Deserialize for BytesMut {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        let declared = decoder.read_varint_u64()?;
        let len = crate::codec::guard_element_count(declared, decoder)?;
        let bytes: Cow<'_, [u8]> = decoder.read_byte_cow(len)?;
        Ok(BytesMut::from(&*bytes))
    }
}

/// Appends to the buffer, growing it as needed. Never fails.
impl Encode for BytesMut {
    #[inline]
    fn write_byte(&mut self, byte: u8) -> Result<()> {
        self.put_u8(byte);
        Ok(())
    }

    #[inline]
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.extend_from_slice(bytes);
        Ok(())
    }

    #[inline]
    fn reserve(&mut self, additional: usize) {
        BytesMut::reserve(self, additional);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode, encode};

    #[test]
    fn test_bytes_round_trip_matches_vec_wire() {
        let b = Bytes::from_static(b"abc");
        let bytes = encode(&b).unwrap();
        assert_eq!(bytes, encode(&b"abc".to_vec()).unwrap());
        assert_eq!(decode::<Bytes>(&bytes).unwrap(), b);
        assert_eq!(&decode::<BytesMut>(&bytes).unwrap()[..], b"abc");
        assert_eq!(b.encoded_len().unwrap(), bytes.len());
    }

    #[test]
    fn test_bytes_mut_encode_appends() {
        let mut buf = BytesMut::from(&b"x"[..]);
        300_u32.serialize(&mut buf).unwrap();
        assert_eq!(&buf[..], &[b'x', 0xac, 0x02]);
    }
}
