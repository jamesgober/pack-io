//! `serde` adapter (feature `serde`): put any type that implements
//! `serde::Serialize` / `serde::Deserialize` on the pack-io wire.
//!
//! The adapter maps serde's data model onto the formats in
//! `docs/WIRE_FORMAT.md`, so a serde-derived type encodes to the same bytes
//! as the equivalent `#[derive(pack_io::Serialize)]` type: structs and
//! tuples are their fields concatenated, enums are
//! `varint(variant_index) ++ fields`, sequences and strings are
//! length-prefixed, and maps are written in canonical encoded-key order.
//!
//! **One exception: sets.** serde's data model has no set type, so a
//! `BTreeSet` or `HashSet` reaches the adapter as a plain sequence and is
//! written in iteration order: `Ord` order for a `BTreeSet` (deterministic,
//! but not the native encoded-byte order), and an unspecified, per-process
//! order for a `HashSet` (not deterministic). Annotate set fields with
//! [`#[serde(with = "pack_io::serde::sorted_set")]`](sorted_set) to get the
//! native bytes and canonical-mode order checks. With that, a producer can
//! switch between the two derives without changing a byte.
//!
//! ```
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize, Debug, PartialEq)]
//! enum Shape {
//!     Circle { radius: u32 },
//!     Rect(u32, u32),
//! }
//!
//! let bytes = pack_io::serde::to_vec(&Shape::Rect(3, 4)).unwrap();
//! assert_eq!(bytes, [1, 3, 4]); // varint(1) ++ varint(3) ++ varint(4)
//! let back: Shape = pack_io::serde::from_slice(&bytes).unwrap();
//! assert_eq!(back, Shape::Rect(3, 4));
//! ```
//!
//! ## Entry points
//!
//! | Function | Use |
//! |---|---|
//! | [`to_vec`] | encode into a fresh `Vec<u8>` |
//! | [`to_encoder`] | encode into any [`Encode`] sink (`Encoder`, `IoEncoder`, `BytesMut`, …) |
//! | [`from_slice`] / [`from_slice_with_config`] | decode one value from a whole buffer; `&str` / `&[u8]` fields borrow from it |
//! | [`from_decoder`] | decode the next value from an in-memory [`Decoder`] (borrowing) |
//! | [`from_decode`] | decode the next value from any [`Decode`] source, e.g. an `IoDecoder` (owning) |
//! | [`compat`] | `#[pack_io(with = "pack_io::serde::compat")]` for one serde-typed field in a pack-io-derived type |
//! | [`sorted_set`] | `#[serde(with = "pack_io::serde::sorted_set")]` for a set field: native set bytes and canonical order checks |
//! | [`Compat`] | wrapper that makes any serde type a pack-io `Serialize` / `Deserialize` / `DeserializeView` |
//!
//! ## Limits
//!
//! pack-io is not self-describing, so serde features that need the decoder
//! to discover the shape of the data at run time are rejected with
//! [`SerialError::Custom`]: `deserialize_any` (untagged and internally
//! tagged enums, `#[serde(flatten)]`) and `deserialize_ignored_any`.
//! Struct fields are positional; `#[serde(skip)]` works, but renaming or
//! reordering fields changes the wire just like it does for the native
//! derive.
//!
//! Sets without [`sorted_set`] are sequences (see above): their bytes
//! follow iteration order, and canonical mode cannot check their order
//! because nothing on the wire or in serde marks them as sets. Maps are
//! always sorted and always checked.
//!
//! Decoding enforces the same [`Config`] limits as the native path:
//! `max_alloc` on every length prefix and, across the whole decode, on
//! sequence and map elements that occupy no input bytes; `max_depth` on
//! nested sequences, maps, structs, enums and options; and canonical mode
//! (non-canonical varints, unsorted or duplicate map keys, unsorted or
//! duplicate [`sorted_set`] elements) when it is on.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use serde::de::{self, DeserializeOwned, IntoDeserializer};
use serde::ser;

use crate::codec::{
    Config, Decode, Decoder, Encode, Encoder, FreeElements, finish, guard_element_count,
};
use crate::error::{Result, SerialError};
use crate::impls::{KeyOrder, SortedEntries};
use crate::traits::{Deserialize, Serialize};
use crate::view::DeserializeView;

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Encode a serde value into a freshly allocated `Vec<u8>`.
///
/// # Errors
///
/// Whatever the value's `serde::Serialize` impl reports (via
/// [`SerialError::Custom`]), or [`SerialError::Custom`] if it emits a
/// sequence whose length differs from the length it declared.
///
/// # Examples
///
/// ```
/// let bytes = pack_io::serde::to_vec(&(7_u64, "hi")).unwrap();
/// assert_eq!(bytes, pack_io::encode(&(7_u64, "hi")).unwrap());
/// ```
pub fn to_vec<T: ser::Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let mut encoder = Encoder::with_capacity(128);
    to_encoder(value, &mut encoder)?;
    Ok(encoder.into_inner())
}

/// Encode a serde value into any [`Encode`] sink.
///
/// # Errors
///
/// As for [`to_vec`], plus any error the sink reports.
///
/// # Examples
///
/// ```
/// use pack_io::Encoder;
///
/// let mut enc = Encoder::new();
/// pack_io::serde::to_encoder(&[1_u16, 2, 3], &mut enc).unwrap();
/// pack_io::serde::to_encoder(&"tail", &mut enc).unwrap();
/// assert_eq!(enc.as_bytes(), &[1, 2, 3, 4, b't', b'a', b'i', b'l']);
/// ```
pub fn to_encoder<T, E>(value: &T, encoder: &mut E) -> Result<()>
where
    T: ser::Serialize + ?Sized,
    E: Encode + ?Sized,
{
    value.serialize(&mut Serializer { encoder })
}

/// Decode one serde value from `bytes`, which must be fully consumed.
/// Borrowed `&str` / `&[u8]` fields point into `bytes`.
///
/// # Errors
///
/// The same errors as [`crate::decode`], plus [`SerialError::Custom`] for
/// the serde features this format cannot support (see the module docs) and
/// for errors raised by the type's `serde::Deserialize` impl.
///
/// # Examples
///
/// ```
/// #[derive(serde::Deserialize)]
/// struct Msg<'a> { id: u32, text: &'a str }
///
/// let bytes = pack_io::encode(&(9_u32, "borrowed")).unwrap();
/// let msg: Msg<'_> = pack_io::serde::from_slice(&bytes).unwrap();
/// assert_eq!((msg.id, msg.text), (9, "borrowed"));
/// ```
pub fn from_slice<'de, T: de::Deserialize<'de>>(bytes: &'de [u8]) -> Result<T> {
    let mut decoder = Decoder::new(bytes);
    let value = from_decoder(&mut decoder)?;
    finish(&decoder, value)
}

/// [`from_slice`] under an explicit [`Config`].
///
/// # Errors
///
/// As for [`from_slice`], plus any error [`Decoder::with_config`] reports
/// for an invalid `config`.
///
/// # Examples
///
/// ```
/// use pack_io::{Config, SerialError};
///
/// // `[0x80, 0x00]` is a non-canonical encoding of 0.
/// let strict = Config::new().with_canonical(true);
/// let err = pack_io::serde::from_slice_with_config::<u32>(&[0x80, 0x00], strict).unwrap_err();
/// assert!(matches!(err, SerialError::NonCanonical { .. }));
/// ```
pub fn from_slice_with_config<'de, T: de::Deserialize<'de>>(
    bytes: &'de [u8],
    config: Config,
) -> Result<T> {
    let mut decoder = Decoder::with_config(bytes, config)?;
    let value = from_decoder(&mut decoder)?;
    finish(&decoder, value)
}

/// Decode the next serde value from an in-memory [`Decoder`], leaving the
/// cursor after it. Borrowed fields point into the decoder's input.
///
/// # Errors
///
/// As for [`from_slice`], except that trailing input is not an error.
///
/// # Examples
///
/// ```
/// use pack_io::{Decoder, Encoder};
///
/// let mut enc = Encoder::new();
/// enc.write(&1_u8).unwrap();
/// enc.write(&"two").unwrap();
/// let bytes = enc.into_inner();
///
/// let mut dec = Decoder::new(&bytes);
/// let a: u8 = pack_io::serde::from_decoder(&mut dec).unwrap();
/// let b: &str = pack_io::serde::from_decoder(&mut dec).unwrap();
/// assert_eq!((a, b), (1, "two"));
/// ```
pub fn from_decoder<'de, T: de::Deserialize<'de>>(decoder: &mut Decoder<'de>) -> Result<T> {
    T::deserialize(&mut Deserializer::new(decoder))
}

/// Decode the next serde value from any [`Decode`] source, such as an
/// `IoDecoder` (feature `std`). Strings and byte buffers are copied, so
/// `T` must own its data.
///
/// # Errors
///
/// As for [`from_decoder`], plus any error the source reports.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "std")] {
/// use std::io::Cursor;
/// use pack_io::IoDecoder;
///
/// let bytes = pack_io::encode(&vec![String::from("a"), String::from("b")]).unwrap();
/// let mut dec = IoDecoder::new(Cursor::new(bytes));
/// let v: Vec<String> = pack_io::serde::from_decode(&mut dec).unwrap();
/// assert_eq!(v, ["a", "b"]);
/// # }
/// ```
pub fn from_decode<T, D>(decoder: &mut D) -> Result<T>
where
    T: DeserializeOwned,
    D: Decode + ?Sized,
{
    let mut owned = Owned(decoder);
    T::deserialize(&mut Deserializer::new(&mut owned))
}

/// Helpers for one serde-typed field inside a pack-io-derived type:
///
/// ```
/// # #[cfg(feature = "derive")] {
/// use pack_io::{Deserialize, Serialize};
///
/// // Any type that only implements serde's traits.
/// #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
/// struct Point { x: i32, y: i32 }
///
/// #[derive(Serialize, Deserialize, Debug, PartialEq)]
/// struct Record {
///     id: u64,
///     #[pack_io(with = "pack_io::serde::compat")]
///     at: Point,
/// }
///
/// let rec = Record { id: 1, at: Point { x: -2, y: 5 } };
/// let bytes = pack_io::encode(&rec).unwrap();
/// assert_eq!(pack_io::decode::<Record>(&bytes).unwrap(), rec);
/// # }
/// ```
pub mod compat {
    use super::{Decode, Decoder, DeserializeOwned, Encode, Result, de, ser};

    /// Encode `value` through serde. See [`super::to_encoder`].
    ///
    /// # Errors
    ///
    /// As for [`super::to_encoder`].
    pub fn serialize<T, E>(value: &T, encoder: &mut E) -> Result<()>
    where
        T: ser::Serialize + ?Sized,
        E: Encode + ?Sized,
    {
        super::to_encoder(value, encoder)
    }

    /// Decode an owned value through serde. See [`super::from_decode`].
    ///
    /// # Errors
    ///
    /// As for [`super::from_decode`].
    pub fn deserialize<T, D>(decoder: &mut D) -> Result<T>
    where
        T: DeserializeOwned,
        D: Decode + ?Sized,
    {
        super::from_decode(decoder)
    }

    /// Decode a possibly-borrowing value through serde, for
    /// `#[derive(DeserializeView)]` types. See [`super::from_decoder`].
    ///
    /// # Errors
    ///
    /// As for [`super::from_decoder`].
    pub fn deserialize_view<'de, T: de::Deserialize<'de>>(decoder: &mut Decoder<'de>) -> Result<T> {
        super::from_decoder(decoder)
    }
}

/// serde `with` helpers for a set field (`BTreeSet`, `HashSet`, or any
/// collection that iterates as `&T` and deserializes from a sequence):
/// `#[serde(with = "pack_io::serde::sorted_set")]`.
///
/// serde has no set type, so without this a set is an ordinary sequence in
/// iteration order. With it:
///
/// - **Encoding** writes the elements sorted by their pack-io encoding,
///   exactly like the native `BTreeSet` / `HashSet` impls, so the bytes are
///   deterministic (also for a `HashSet`) and identical to a
///   `#[derive(pack_io::Serialize)]` type with the same field.
/// - **Decoding** in canonical mode ([`Config::with_canonical`]) rejects
///   elements that are not in strictly increasing encoded order (which
///   also rejects duplicates) with
///   `SerialError::NonCanonical { reason: "set element order" }`.
///
/// Other serde formats see a newtype around an ordinary sequence: the
/// elements are still sorted, so their output is deterministic too.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "std")] {
/// use std::collections::{BTreeSet, HashSet};
///
/// #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
/// struct Tags {
///     #[serde(with = "pack_io::serde::sorted_set")]
///     names: BTreeSet<String>,
///     #[serde(with = "pack_io::serde::sorted_set")]
///     ids: HashSet<u32>,
/// }
///
/// let tags = Tags {
///     names: ["b", "aa", "ccc"].map(String::from).into(),
///     ids: [300, 1, 129].into(),
/// };
/// let bytes = pack_io::serde::to_vec(&tags).unwrap();
/// // Byte-identical to the native encoding of the same fields.
/// assert_eq!(bytes, pack_io::encode(&(&tags.names, &tags.ids)).unwrap());
/// assert_eq!(pack_io::serde::from_slice::<Tags>(&bytes).unwrap(), tags);
/// # }
/// ```
pub mod sorted_set {
    use alloc::vec::Vec;
    use core::fmt;
    use core::marker::PhantomData;

    use serde::{de, ser};

    /// The newtype name that tells the pack-io deserializer the sequence
    /// inside is a set, so canonical mode checks its order.
    pub(super) const NAME: &str = "$pack_io::serde::sorted_set";

    /// Serialize `set` with its elements sorted by their pack-io encoding.
    ///
    /// # Errors
    ///
    /// Whatever the serializer or an element's `Serialize` impl reports.
    pub fn serialize<'a, C, T, S>(set: &'a C, serializer: S) -> Result<S::Ok, S::Error>
    where
        &'a C: IntoIterator<Item = &'a T>,
        T: ser::Serialize + 'a,
        S: ser::Serializer,
    {
        let mut keyed: Vec<(Vec<u8>, &'a T)> = Vec::new();
        for item in set {
            let key = super::to_vec(item).map_err(ser::Error::custom)?;
            keyed.push((key, item));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        serializer.serialize_newtype_struct(NAME, &Sorted(keyed))
    }

    /// Deserialize a set written by [`serialize`] (or any sequence). In
    /// canonical mode the pack-io deserializer checks the element order.
    ///
    /// # Errors
    ///
    /// Whatever the deserializer or the collection's `Deserialize` impl
    /// reports.
    pub fn deserialize<'de, C, D>(deserializer: D) -> Result<C, D::Error>
    where
        C: de::Deserialize<'de>,
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_newtype_struct(NAME, SetVisitor(PhantomData))
    }

    /// The sorted elements, serialized as a sequence.
    struct Sorted<'a, T>(Vec<(Vec<u8>, &'a T)>);

    impl<T: ser::Serialize> ser::Serialize for Sorted<'_, T> {
        fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.collect_seq(self.0.iter().map(|(_, item)| *item))
        }
    }

    struct SetVisitor<C>(PhantomData<C>);

    impl<'de, C: de::Deserialize<'de>> de::Visitor<'de> for SetVisitor<C> {
        type Value = C;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a set")
        }

        fn visit_newtype_struct<D: de::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<C, D::Error> {
            C::deserialize(deserializer)
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<C, A::Error> {
            C::deserialize(de::value::SeqAccessDeserializer::new(seq))
        }
    }
}

/// Wrapper that gives any serde type pack-io's [`Serialize`],
/// [`Deserialize`] and [`DeserializeView`] impls, so it can sit anywhere a
/// pack-io type can: inside a `Vec`, a map, a tuple, or a derived struct.
///
/// The wrapper adds no bytes: `Compat(v)` encodes exactly like
/// `pack_io::serde::to_vec(&v)`. Sets inside `v` follow the adapter's rules
/// (see the [module docs](self) and [`sorted_set`]).
///
/// # Examples
///
/// ```
/// use pack_io::serde::Compat;
///
/// #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
/// struct Tag { key: String, value: String }
///
/// let tags = vec![Compat(Tag { key: "env".into(), value: "prod".into() })];
/// let bytes = pack_io::encode(&tags).unwrap();
/// let back: Vec<Compat<Tag>> = pack_io::decode(&bytes).unwrap();
/// assert_eq!(back[0].0.value, "prod");
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Compat<T>(pub T);

impl<T: ser::Serialize> Serialize for Compat<T> {
    #[inline]
    fn serialize<E: Encode + ?Sized>(&self, encoder: &mut E) -> Result<()> {
        to_encoder(&self.0, encoder)
    }
}

impl<T: DeserializeOwned> Deserialize for Compat<T> {
    #[inline]
    fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<Self> {
        from_decode(decoder).map(Compat)
    }
}

impl<'de, T: de::Deserialize<'de>> DeserializeView<'de> for Compat<T> {
    #[inline]
    fn deserialize_view(decoder: &mut Decoder<'de>) -> Result<Self> {
        from_decoder(decoder).map(Compat)
    }
}

// ---------------------------------------------------------------------------
// Serializer
// ---------------------------------------------------------------------------

/// serde `Serializer` writing pack-io wire bytes into an [`Encode`] sink.
struct Serializer<'e, E: ?Sized> {
    encoder: &'e mut E,
}

impl<E: Encode + ?Sized> Serializer<'_, E> {
    #[inline]
    fn write_len(&mut self, len: usize) -> Result<()> {
        self.encoder.write_varint_u64(len as u64)
    }

    #[inline]
    fn write_variant(&mut self, index: u32) -> Result<()> {
        self.encoder.write_varint_u64(u64::from(index))
    }
}

impl<'a, 'e, E: Encode + ?Sized> ser::Serializer for &'a mut Serializer<'e, E> {
    type Ok = ();
    type Error = SerialError;
    type SerializeSeq = SeqSerializer<'a, 'e, E>;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = MapSerializer<'a, 'e, E>;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_i128(self, v: i128) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_u128(self, v: u128) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_char(self, v: char) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        v.serialize(self.encoder)
    }

    fn serialize_none(self) -> Result<()> {
        self.encoder.write_byte(0x00)
    }

    fn serialize_some<T: ser::Serialize + ?Sized>(self, value: &T) -> Result<()> {
        self.encoder.write_byte(0x01)?;
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        self.write_variant(variant_index)
    }

    fn serialize_newtype_struct<T: ser::Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ser::Serialize + ?Sized>(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<()> {
        self.write_variant(variant_index)?;
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SeqSerializer<'a, 'e, E>> {
        Ok(match len {
            Some(expected) => {
                self.write_len(expected)?;
                SeqSerializer::Direct {
                    ser: self,
                    expected,
                    written: 0,
                }
            }
            None => SeqSerializer::Buffered {
                ser: self,
                scratch: Encoder::new(),
                count: 0,
            },
        })
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self> {
        Ok(self)
    }

    fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<Self> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self> {
        self.write_variant(variant_index)?;
        Ok(self)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<MapSerializer<'a, 'e, E>> {
        Ok(MapSerializer {
            ser: self,
            entries: SortedEntries::with_capacity(len.unwrap_or(0)),
        })
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self> {
        self.write_variant(variant_index)?;
        Ok(self)
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

/// Sequence serializer. A known length is written up front and checked at
/// the end; an unknown length buffers the elements and writes the count
/// once it is known.
enum SeqSerializer<'a, 'e, E: ?Sized> {
    Direct {
        ser: &'a mut Serializer<'e, E>,
        expected: usize,
        written: usize,
    },
    Buffered {
        ser: &'a mut Serializer<'e, E>,
        scratch: Encoder,
        count: usize,
    },
}

impl<E: Encode + ?Sized> ser::SerializeSeq for SeqSerializer<'_, '_, E> {
    type Ok = ();
    type Error = SerialError;

    fn serialize_element<T: ser::Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        match self {
            Self::Direct { ser, written, .. } => {
                *written = written.saturating_add(1);
                value.serialize(&mut **ser)
            }
            Self::Buffered { scratch, count, .. } => {
                *count = count.saturating_add(1);
                value.serialize(&mut Serializer { encoder: scratch })
            }
        }
    }

    fn end(self) -> Result<()> {
        match self {
            Self::Direct {
                expected, written, ..
            } => {
                if expected != written {
                    return Err(SerialError::custom(
                        "serde sequence emitted a different number of elements than it declared",
                    ));
                }
                Ok(())
            }
            Self::Buffered {
                ser,
                scratch,
                count,
            } => {
                ser.write_len(count)?;
                ser.encoder.write_bytes(scratch.as_bytes())
            }
        }
    }
}

/// Map serializer: entries are buffered and written in canonical
/// encoded-key order, exactly like the native map impls.
struct MapSerializer<'a, 'e, E: ?Sized> {
    ser: &'a mut Serializer<'e, E>,
    entries: SortedEntries,
}

impl<E: Encode + ?Sized> ser::SerializeMap for MapSerializer<'_, '_, E> {
    type Ok = ();
    type Error = SerialError;

    fn serialize_key<T: ser::Serialize + ?Sized>(&mut self, key: &T) -> Result<()> {
        self.entries.begin_entry();
        key.serialize(&mut Serializer {
            encoder: self.entries.encoder(),
        })?;
        self.entries.end_key();
        Ok(())
    }

    fn serialize_value<T: ser::Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut Serializer {
            encoder: self.entries.encoder(),
        })?;
        self.entries.end_entry();
        Ok(())
    }

    fn end(self) -> Result<()> {
        self.entries.write_to(self.ser.encoder)
    }
}

/// Tuples, tuple structs, structs and their enum-variant forms: fields are
/// written in order with no framing.
macro_rules! impl_fields_serializer {
    ($($trait:ident :: $method:ident $(, $key:ty)?);+ $(;)?) => {
        $(
            impl<E: Encode + ?Sized> ser::$trait for &mut Serializer<'_, E> {
                type Ok = ();
                type Error = SerialError;

                fn $method<T: ser::Serialize + ?Sized>(
                    &mut self,
                    $(_key: $key,)?
                    value: &T,
                ) -> Result<()> {
                    value.serialize(&mut **self)
                }

                fn end(self) -> Result<()> {
                    Ok(())
                }
            }
        )+
    };
}

impl_fields_serializer! {
    SerializeTuple::serialize_element;
    SerializeTupleStruct::serialize_field;
    SerializeTupleVariant::serialize_field;
    SerializeStruct::serialize_field, &'static str;
    SerializeStructVariant::serialize_field, &'static str;
}

// ---------------------------------------------------------------------------
// Deserializer
// ---------------------------------------------------------------------------

/// Where the deserializer gets its bytes: borrowed from an in-memory
/// [`Decoder`], or copied out of any other [`Decode`] via [`Owned`].
trait Source<'de>: Decode {
    /// Read `len` bytes of UTF-8 and hand them to `visitor`.
    fn visit_str<V: de::Visitor<'de>>(&mut self, len: usize, visitor: V) -> Result<V::Value>;

    /// Read `len` raw bytes and hand them to `visitor`.
    fn visit_bytes<V: de::Visitor<'de>>(&mut self, len: usize, visitor: V) -> Result<V::Value>;
}

impl<'de> Source<'de> for Decoder<'de> {
    fn visit_str<V: de::Visitor<'de>>(&mut self, len: usize, visitor: V) -> Result<V::Value> {
        let bytes = self.read_borrowed(len)?;
        let text = core::str::from_utf8(bytes).map_err(|_| SerialError::InvalidUtf8)?;
        visitor.visit_borrowed_str(text)
    }

    fn visit_bytes<V: de::Visitor<'de>>(&mut self, len: usize, visitor: V) -> Result<V::Value> {
        visitor.visit_borrowed_bytes(self.read_borrowed(len)?)
    }
}

/// Adapts any [`Decode`] into a [`Source`] that copies strings and bytes.
struct Owned<'d, D: ?Sized>(&'d mut D);

impl<D: Decode + ?Sized> Decode for Owned<'_, D> {
    #[inline]
    fn read_byte(&mut self) -> Result<u8> {
        self.0.read_byte()
    }

    #[inline]
    fn read_into(&mut self, out: &mut [u8]) -> Result<()> {
        self.0.read_into(out)
    }

    #[inline]
    fn max_alloc(&self) -> usize {
        self.0.max_alloc()
    }

    #[inline]
    fn remaining_hint(&self) -> Option<usize> {
        self.0.remaining_hint()
    }

    #[inline]
    fn config(&self) -> Config {
        self.0.config()
    }

    #[inline]
    fn depth(&self) -> usize {
        self.0.depth()
    }

    #[inline]
    fn enter_nested(&mut self) -> Result<()> {
        self.0.enter_nested()
    }

    #[inline]
    fn leave_nested(&mut self) {
        self.0.leave_nested();
    }

    #[inline]
    fn consumed(&self) -> Option<&[u8]> {
        self.0.consumed()
    }

    #[inline]
    fn bytes_read(&self) -> Option<u64> {
        self.0.bytes_read()
    }

    #[inline]
    fn unbacked(&self) -> usize {
        self.0.unbacked()
    }

    #[inline]
    fn charge_unbacked(&mut self, bytes: usize) -> Result<()> {
        self.0.charge_unbacked(bytes)
    }

    #[inline]
    fn read_byte_cow(&mut self, len: usize) -> Result<Cow<'_, [u8]>> {
        self.0.read_byte_cow(len)
    }

    #[inline]
    fn read_varint_u64(&mut self) -> Result<u64> {
        self.0.read_varint_u64()
    }

    #[inline]
    fn read_varint_u128(&mut self) -> Result<u128> {
        self.0.read_varint_u128()
    }

    #[inline]
    fn read_length_prefixed(&mut self) -> Result<Vec<u8>> {
        self.0.read_length_prefixed()
    }
}

impl<'de, D: Decode + ?Sized> Source<'de> for Owned<'_, D> {
    fn visit_str<V: de::Visitor<'de>>(&mut self, len: usize, visitor: V) -> Result<V::Value> {
        let bytes = self.0.read_byte_cow(len)?.into_owned();
        let text = String::from_utf8(bytes).map_err(|_| SerialError::InvalidUtf8)?;
        visitor.visit_string(text)
    }

    fn visit_bytes<V: de::Visitor<'de>>(&mut self, len: usize, visitor: V) -> Result<V::Value> {
        visitor.visit_byte_buf(self.0.read_byte_cow(len)?.into_owned())
    }
}

/// serde `Deserializer` reading pack-io wire bytes from a [`Source`].
struct Deserializer<'s, S: ?Sized> {
    source: &'s mut S,
    /// Set by a [`sorted_set`] newtype: the next sequence is a set whose
    /// element order canonical mode checks.
    set_next: bool,
}

impl<'s, S: ?Sized> Deserializer<'s, S> {
    fn new(source: &'s mut S) -> Self {
        Self {
            source,
            set_next: false,
        }
    }
}

impl<'de, S: Source<'de> + ?Sized> Deserializer<'_, S> {
    /// Read and validate a length / count prefix.
    fn read_len(&mut self) -> Result<usize> {
        let declared = self.source.read_varint_u64()?;
        guard_element_count(declared, &*self.source)
    }

    /// Run `body` one nesting level deeper.
    fn nested<T>(&mut self, body: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.source.enter_nested()?;
        let out = body(self);
        self.source.leave_nested();
        out
    }
}

/// [`SerialError::Custom`] for a serde feature this format cannot support.
fn unsupported<T>(what: &'static str) -> Result<T> {
    Err(SerialError::Custom {
        message: Cow::Borrowed(what),
    })
}

/// Forward a primitive `deserialize_*` to pack-io's native impl and the
/// matching `visit_*`.
macro_rules! forward_primitive {
    ($($method:ident => $ty:ty, $visit:ident;)+) => {
        $(
            fn $method<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
                let value = <$ty as Deserialize>::deserialize(&mut *self.source)?;
                visitor.$visit(value)
            }
        )+
    };
}

impl<'de, S: Source<'de> + ?Sized> de::Deserializer<'de> for &mut Deserializer<'_, S> {
    type Error = SerialError;

    fn deserialize_any<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unsupported(
            "pack-io is not self-describing: deserialize_any (untagged / internally tagged enums, \
             #[serde(flatten)]) is not supported",
        )
    }

    forward_primitive! {
        deserialize_bool => bool, visit_bool;
        deserialize_i8 => i8, visit_i8;
        deserialize_i16 => i16, visit_i16;
        deserialize_i32 => i32, visit_i32;
        deserialize_i64 => i64, visit_i64;
        deserialize_i128 => i128, visit_i128;
        deserialize_u8 => u8, visit_u8;
        deserialize_u16 => u16, visit_u16;
        deserialize_u32 => u32, visit_u32;
        deserialize_u64 => u64, visit_u64;
        deserialize_u128 => u128, visit_u128;
        deserialize_f32 => f32, visit_f32;
        deserialize_f64 => f64, visit_f64;
        deserialize_char => char, visit_char;
    }

    fn deserialize_str<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let len = self.read_len()?;
        self.source.visit_str(len, visitor)
    }

    fn deserialize_string<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let len = self.read_len()?;
        self.source.visit_bytes(len, visitor)
    }

    fn deserialize_byte_buf<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.source.read_byte()? {
            0x00 => visitor.visit_none(),
            0x01 => self.nested(|de| visitor.visit_some(&mut *de)),
            tag => Err(SerialError::InvalidTag {
                kind: "Option",
                tag,
            }),
        }
    }

    fn deserialize_unit<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        if name == sorted_set::NAME {
            // Transparent on the wire, like the native set impls (no extra
            // nesting level); the flag reaches the `deserialize_seq` the
            // set's own `Deserialize` makes next.
            self.set_next = true;
            let out = visitor.visit_newtype_struct(&mut *self);
            self.set_next = false;
            return out;
        }
        self.nested(|de| visitor.visit_newtype_struct(&mut *de))
    }

    fn deserialize_seq<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let is_set = core::mem::take(&mut self.set_next);
        let len = self.read_len()?;
        let order =
            (is_set && self.source.config().canonical).then(|| KeyOrder::new("set element order"));
        self.nested(|de| {
            let free = Some(FreeElements::new(&*de.source));
            let mut access = SeqAccess {
                de,
                remaining: len,
                free,
                order,
            };
            let value = visitor.visit_seq(&mut access)?;
            if access.remaining != 0 {
                return unsupported("serde visitor stopped before the end of the sequence");
            }
            Ok(value)
        })
    }

    fn deserialize_tuple<V: de::Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value> {
        self.visit_exact_seq(len, visitor)
    }

    fn deserialize_tuple_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value> {
        self.visit_exact_seq(len, visitor)
    }

    fn deserialize_map<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let len = self.read_len()?;
        self.nested(|de| {
            let order = de
                .source
                .config()
                .canonical
                .then(|| KeyOrder::new("map key order"));
            let free = FreeElements::new(&*de.source);
            let mut access = MapAccess {
                de,
                remaining: len,
                order,
                free,
                key_size: 0,
            };
            let value = visitor.visit_map(&mut access)?;
            if access.remaining != 0 {
                return unsupported("serde visitor stopped before the end of the map");
            }
            Ok(value)
        })
    }

    fn deserialize_struct<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.visit_exact_seq(fields.len(), visitor)
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.nested(|de| visitor.visit_enum(&mut *de))
    }

    fn deserialize_identifier<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let index = <u32 as Deserialize>::deserialize(&mut *self.source)?;
        visitor.visit_u32(index)
    }

    fn deserialize_ignored_any<V: de::Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unsupported("pack-io is not self-describing: values cannot be skipped without their type")
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

impl<'de, S: Source<'de> + ?Sized> Deserializer<'_, S> {
    /// Visit exactly `len` positional elements (a length-prefixed sequence,
    /// a tuple, or a struct's fields).
    fn visit_exact_seq<V: de::Visitor<'de>>(&mut self, len: usize, visitor: V) -> Result<V::Value> {
        self.nested(|de| {
            let mut access = SeqAccess {
                de,
                remaining: len,
                free: None,
                order: None,
            };
            let value = visitor.visit_seq(&mut access)?;
            if access.remaining != 0 {
                return unsupported("serde visitor stopped before the end of the sequence");
            }
            Ok(value)
        })
    }
}

struct SeqAccess<'a, 's, S: ?Sized> {
    de: &'a mut Deserializer<'s, S>,
    remaining: usize,
    /// Charging of elements that occupy no input, for sequences whose
    /// length came off the wire (`None` for tuples and structs).
    free: Option<FreeElements>,
    /// Element order check for a [`sorted_set`] in canonical mode.
    order: Option<KeyOrder>,
}

impl<'de, S: Source<'de> + ?Sized> de::SeqAccess<'de> for SeqAccess<'_, '_, S> {
    type Error = SerialError;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        let start = match self.order.as_ref() {
            Some(order) => Some(order.mark(&*self.de.source)?),
            None => None,
        };
        let value = seed.deserialize(&mut *self.de)?;
        if let (Some(order), Some(start)) = (self.order.as_mut(), start) {
            order.check(&*self.de.source, start)?;
        }
        if let Some(free) = self.free.as_mut() {
            free.after_element(&mut *self.de.source, core::mem::size_of::<T::Value>())?;
        }
        Ok(Some(value))
    }

    fn size_hint(&self) -> Option<usize> {
        // Never let a hostile count drive a visitor's pre-allocation past
        // what the input can back.
        let by_input = self.de.source.remaining_hint().unwrap_or(4096);
        Some(self.remaining.min(by_input))
    }
}

struct MapAccess<'a, 's, S: ?Sized> {
    de: &'a mut Deserializer<'s, S>,
    remaining: usize,
    order: Option<KeyOrder>,
    /// Charging of entries that occupy no input.
    free: FreeElements,
    /// In-memory size of the key just read, charged with its value.
    key_size: usize,
}

impl<'de, S: Source<'de> + ?Sized> de::MapAccess<'de> for MapAccess<'_, '_, S> {
    type Error = SerialError;

    fn next_key_seed<K: de::DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        self.key_size = core::mem::size_of::<K::Value>();
        let Some(order) = self.order.as_mut() else {
            return seed.deserialize(&mut *self.de).map(Some);
        };
        let start = order.mark(&*self.de.source)?;
        let key = seed.deserialize(&mut *self.de)?;
        order.check(&*self.de.source, start)?;
        Ok(Some(key))
    }

    fn next_value_seed<V: de::DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        let value = seed.deserialize(&mut *self.de)?;
        let entry = self
            .key_size
            .saturating_add(core::mem::size_of::<V::Value>());
        self.free.after_element(&mut *self.de.source, entry)?;
        Ok(value)
    }

    fn size_hint(&self) -> Option<usize> {
        let by_input = self.de.source.remaining_hint().unwrap_or(4096);
        Some(self.remaining.min(by_input))
    }
}

impl<'de, S: Source<'de> + ?Sized> de::EnumAccess<'de> for &mut Deserializer<'_, S> {
    type Error = SerialError;
    type Variant = Self;

    fn variant_seed<V: de::DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self)> {
        let tag = self.source.read_varint_u64()?;
        let index = u32::try_from(tag).map_err(|_| SerialError::UnknownVariant {
            kind: "serde enum",
            index: tag,
        })?;
        let deserializer: de::value::U32Deserializer<SerialError> = index.into_deserializer();
        let value = seed.deserialize(deserializer)?;
        Ok((value, self))
    }
}

impl<'de, S: Source<'de> + ?Sized> de::VariantAccess<'de> for &mut Deserializer<'_, S> {
    type Error = SerialError;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(self)
    }

    fn tuple_variant<V: de::Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value> {
        self.visit_exact_seq(len, visitor)
    }

    fn struct_variant<V: de::Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.visit_exact_seq(fields.len(), visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;
    use alloc::vec;

    #[derive(::serde::Serialize, ::serde::Deserialize, Debug, PartialEq)]
    struct Inner {
        a: u8,
        b: Option<i64>,
    }

    #[test]
    fn test_round_trip_struct_with_map_and_seq() {
        let mut map = BTreeMap::new();
        let _ = map.insert("k".to_string(), vec![1u32, 300]);
        let value = (Inner { a: 7, b: Some(-9) }, map, 'x');
        let bytes = to_vec(&value).unwrap();
        let back: (Inner, BTreeMap<String, Vec<u32>>, char) = from_slice(&bytes).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn test_any_is_rejected() {
        #[derive(Debug)]
        struct Any;
        impl<'de> de::Deserialize<'de> for Any {
            fn deserialize<D: de::Deserializer<'de>>(d: D) -> core::result::Result<Self, D::Error> {
                d.deserialize_any(de::IgnoredAny).map(|_| Any)
            }
        }
        let err = from_slice::<Any>(&[0]).unwrap_err();
        assert!(matches!(err, SerialError::Custom { .. }));
    }
}
