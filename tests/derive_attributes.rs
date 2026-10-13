//! Derive attributes added in 2.0: `skip`, `default`, `with`, `crate`,
//! explicit enum `index`, plus the derive fixes for raw identifiers
//! (PACK-013), identifier hygiene (PACK-023), lifetime-less
//! `DeserializeView` (PACK-020) and `encoded_len`.

use std::collections::BTreeMap;

use pack_io::{
    Decode, DeserializeView, Encode, SerialError, Serialize, decode, decode_view, encode,
};

/// Re-export path used by the `crate = "..."` test, the way a facade crate
/// such as `network-protocol` would re-export pack-io.
mod facade {
    pub use pack_io as codec;
}

// ---------------------------------------------------------------------------
// skip / default
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
struct WithCache {
    id: u32,
    #[pack_io(skip)]
    cache: Vec<u8>,
    #[pack_io(skip, default = "default_label")]
    label: String,
    name: String,
}

fn default_label() -> String {
    String::from("unset")
}

#[test]
fn test_skipped_fields_are_not_encoded_and_decode_to_defaults() {
    let value = WithCache {
        id: 7,
        cache: vec![1, 2, 3],
        label: String::from("local only"),
        name: String::from("n"),
    };
    let bytes = encode(&value).unwrap();
    assert_eq!(bytes, encode(&(7u32, "n")).unwrap());
    let back: WithCache = decode(&bytes).unwrap();
    assert_eq!(
        back,
        WithCache {
            id: 7,
            cache: vec![],
            label: String::from("unset"),
            name: String::from("n"),
        }
    );
    assert_eq!(value.encoded_len().unwrap(), bytes.len());
}

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 2)]
struct Settings {
    volume: u8,
    #[pack_io(since = 2, default = "default_theme")]
    theme: String,
}

fn default_theme() -> String {
    String::from("dark")
}

#[test]
fn test_default_path_fills_fields_missing_from_older_payloads() {
    // A version-1 payload: varint(1) ++ varint(1) ++ volume.
    let v1 = [1, 1, 40];
    let back: Settings = decode(&v1).unwrap();
    assert_eq!(back.theme, "dark");
    let current = Settings {
        volume: 1,
        theme: String::from("light"),
    };
    assert_eq!(
        decode::<Settings>(&encode(&current).unwrap()).unwrap(),
        current
    );
}

// ---------------------------------------------------------------------------
// with
// ---------------------------------------------------------------------------

/// Encode a `u32` as four little-endian bytes instead of a varint.
mod fixed_u32 {
    use pack_io::{Decode, Decoder, Encode, Result};

    pub fn serialize<E: Encode + ?Sized>(value: &u32, encoder: &mut E) -> Result<()> {
        encoder.write_bytes(&value.to_le_bytes())
    }

    pub fn deserialize<D: Decode + ?Sized>(decoder: &mut D) -> Result<u32> {
        let mut buf = [0u8; 4];
        decoder.read_into(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    pub fn deserialize_view(decoder: &mut Decoder<'_>) -> Result<u32> {
        deserialize(decoder)
    }
}

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize, DeserializeView)]
struct Header {
    #[pack_io(with = "fixed_u32")]
    magic: u32,
    len: u32,
}

#[test]
fn test_with_module_controls_the_field_encoding() {
    let h = Header {
        magic: 0xCAFE_BABE,
        len: 3,
    };
    let bytes = encode(&h).unwrap();
    assert_eq!(bytes, [0xBE, 0xBA, 0xFE, 0xCA, 3]);
    assert_eq!(decode::<Header>(&bytes).unwrap(), h);
    assert_eq!(decode_view::<Header>(&bytes).unwrap(), h);
    assert_eq!(h.encoded_len().unwrap(), 5);
}

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 1)]
struct VersionedWith {
    #[pack_io(with = "fixed_u32")]
    magic: u32,
}

#[test]
fn test_with_fields_are_counted_exactly_in_versioned_frames() {
    let bytes = encode(&VersionedWith { magic: 1 }).unwrap();
    assert_eq!(bytes, [1, 4, 1, 0, 0, 0]);
    assert_eq!(decode::<VersionedWith>(&bytes).unwrap().magic, 1);
}

/// Box support through `with`, the documented route for recursive types.
mod boxed {
    use pack_io::{Decode, Deserialize, Encode, Result, Serialize};

    pub fn serialize<T: Serialize, E: Encode + ?Sized>(
        v: &Option<Box<T>>,
        e: &mut E,
    ) -> Result<()> {
        v.as_deref().serialize(e)
    }

    pub fn deserialize<T: Deserialize, D: Decode + ?Sized>(d: &mut D) -> Result<Option<Box<T>>> {
        Ok(Option::<T>::deserialize(d)?.map(Box::new))
    }
}

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
struct List {
    value: u8,
    #[pack_io(with = "boxed")]
    next: Option<Box<List>>,
}

#[test]
fn test_boxed_recursion_through_with() {
    let list = List {
        value: 1,
        next: Some(Box::new(List {
            value: 2,
            next: None,
        })),
    };
    let bytes = encode(&list).unwrap();
    assert_eq!(bytes, [1, 1, 2, 0]);
    assert_eq!(decode::<List>(&bytes).unwrap(), list);
    let hostile = [1u8, 1].repeat(100_000);
    assert!(matches!(
        decode::<List>(&hostile),
        Err(SerialError::DepthLimitExceeded { .. })
    ));
}

// ---------------------------------------------------------------------------
// crate = "..."
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, facade::codec::Serialize, facade::codec::Deserialize)]
#[pack_io(crate = "facade::codec")]
struct ThroughFacade {
    id: u64,
}

#[derive(Debug, PartialEq, facade::codec::Serialize, facade::codec::DeserializeView)]
#[pack_io(crate = "crate::facade::codec", version = 1)]
struct ViewThroughFacade<'a> {
    name: &'a str,
}

#[test]
fn test_crate_attribute_compiles_against_a_reexport() {
    let bytes = encode(&ThroughFacade { id: 9 }).unwrap();
    assert_eq!(decode::<ThroughFacade>(&bytes).unwrap().id, 9);
    let bytes = encode(&ViewThroughFacade { name: "x" }).unwrap();
    assert_eq!(
        decode_view::<ViewThroughFacade<'_>>(&bytes).unwrap().name,
        "x"
    );
}

// ---------------------------------------------------------------------------
// Explicit enum indices
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize, DeserializeView)]
enum Command {
    #[pack_io(index = 10)]
    Start,
    // Implicit: previous + 1 = 11.
    Stop {
        code: u8,
    },
    #[pack_io(index = 2)]
    Pause(u32),
    // Implicit: 3.
    Resume,
}

#[test]
fn test_explicit_indices_and_implicit_continuation() {
    assert_eq!(encode(&Command::Start).unwrap(), [10]);
    assert_eq!(encode(&Command::Stop { code: 4 }).unwrap(), [11, 4]);
    assert_eq!(encode(&Command::Pause(300)).unwrap(), [2, 0xac, 0x02]);
    assert_eq!(encode(&Command::Resume).unwrap(), [3]);
    for c in [
        Command::Start,
        Command::Stop { code: 1 },
        Command::Pause(9),
        Command::Resume,
    ] {
        let bytes = encode(&c).unwrap();
        assert_eq!(decode::<Command>(&bytes).unwrap(), c);
        assert_eq!(decode_view::<Command>(&bytes).unwrap(), c);
        assert_eq!(c.encoded_len().unwrap(), bytes.len());
    }
    // Index 0 is unused now.
    assert_eq!(
        decode::<Command>(&[0]),
        Err(SerialError::UnknownVariant {
            kind: "Command",
            index: 0
        })
    );
}

#[test]
fn test_unknown_variant_reports_the_real_index() {
    // 1.0 reported u64::MAX for tags above u32::MAX.
    let tag = encode(&(u64::from(u32::MAX) + 5)).unwrap();
    assert_eq!(
        decode::<Command>(&tag),
        Err(SerialError::UnknownVariant {
            kind: "Command",
            index: u64::from(u32::MAX) + 5,
        })
    );
}

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
enum WithSkippedField {
    A {
        keep: u8,
        #[pack_io(skip)]
        scratch: Vec<u8>,
    },
    B(#[pack_io(skip)] u64, String),
}

#[test]
fn test_skip_on_enum_variant_fields() {
    let a = WithSkippedField::A {
        keep: 1,
        scratch: vec![9],
    };
    assert_eq!(encode(&a).unwrap(), [0, 1]);
    let b = WithSkippedField::B(77, String::from("z"));
    assert_eq!(encode(&b).unwrap(), [1, 1, b'z']);
    assert_eq!(
        decode::<WithSkippedField>(&[1, 1, b'z']).unwrap(),
        WithSkippedField::B(0, String::from("z"))
    );
}

// ---------------------------------------------------------------------------
// Raw identifiers and hygiene (PACK-013, PACK-023)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 1)]
struct RawIdents {
    r#type: u32,
    r#match: String,
}

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
#[allow(non_camel_case_types)]
enum r#enum {
    A {
        __encoder: u32,
        __decoder: u32,
        __len: u8,
    },
    B(u8),
}

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize, DeserializeView)]
#[pack_io(version = 1)]
struct Collides {
    __body: u32,
    __version: u32,
    __body_dec: u32,
    __config: u8,
    __pk_f0: u8,
}

#[test]
fn test_raw_identifiers_and_generated_name_collisions() {
    let r = RawIdents {
        r#type: 5,
        r#match: String::from("m"),
    };
    assert_eq!(decode::<RawIdents>(&encode(&r).unwrap()).unwrap(), r);

    let e = r#enum::A {
        __encoder: 1,
        __decoder: 2,
        __len: 3,
    };
    assert_eq!(encode(&e).unwrap(), [0, 1, 2, 3]);
    assert_eq!(decode::<r#enum>(&[0, 1, 2, 3]).unwrap(), e);
    assert_eq!(
        decode::<r#enum>(&[9]),
        Err(SerialError::UnknownVariant {
            kind: "enum",
            index: 9
        })
    );

    let c = Collides {
        __body: 1,
        __version: 2,
        __body_dec: 3,
        __config: 4,
        __pk_f0: 5,
    };
    let bytes = encode(&c).unwrap();
    assert_eq!(decode::<Collides>(&bytes).unwrap(), c);
    assert_eq!(decode_view::<Collides>(&bytes).unwrap(), c);
}

// ---------------------------------------------------------------------------
// DeserializeView on lifetime-less types (PACK-020)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize, DeserializeView)]
struct Id {
    shard: u16,
    seq: u64,
}

#[derive(Debug, PartialEq, pack_io::Serialize, DeserializeView)]
enum Kind {
    Text,
    Blob(u32),
}

#[derive(Debug, PartialEq, pack_io::Serialize, DeserializeView)]
struct Envelope<'a> {
    id: Id,
    kind: Kind,
    body: &'a str,
    meta: BTreeMap<&'a str, Id>,
}

#[test]
fn test_owned_types_nest_inside_borrowing_views() {
    let env = Envelope {
        id: Id { shard: 3, seq: 99 },
        kind: Kind::Blob(7),
        body: "payload",
        meta: BTreeMap::from([("parent", Id { shard: 1, seq: 2 })]),
    };
    let bytes = encode(&env).unwrap();
    let view: Envelope<'_> = decode_view(&bytes).unwrap();
    assert_eq!(view, env);
    // `body` and the map key borrow from `bytes`; nothing was copied.
    let input = bytes.as_ptr_range();
    assert!(input.contains(&view.body.as_ptr()));
    assert!(input.contains(&view.meta.keys().next().unwrap().as_ptr()));
}

// ---------------------------------------------------------------------------
// Generics
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, pack_io::Serialize, pack_io::Deserialize, DeserializeView)]
struct Pair<A, B> {
    a: A,
    b: B,
}

#[derive(Debug, PartialEq, pack_io::Serialize, DeserializeView)]
struct BorrowedPair<'a, T> {
    name: &'a str,
    value: T,
}

#[test]
fn test_generic_derives_in_all_three_modes() {
    let p = Pair {
        a: 1u8,
        b: String::from("b"),
    };
    let bytes = encode(&p).unwrap();
    assert_eq!(decode::<Pair<u8, String>>(&bytes).unwrap(), p);
    let v: Pair<u8, &str> = decode_view(&bytes).unwrap();
    assert_eq!((v.a, v.b), (1, "b"));
    let bp = BorrowedPair {
        name: "n",
        value: Pair { a: 2u16, b: 3i8 },
    };
    let bytes = encode(&bp).unwrap();
    assert_eq!(
        decode_view::<BorrowedPair<'_, Pair<u16, i8>>>(&bytes).unwrap(),
        bp
    );
}

// ---------------------------------------------------------------------------
// encoded_len matches encode() for derived types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 3)]
struct Evolving {
    a: u64,
    #[pack_io(removed = 3)]
    gone: String,
    #[pack_io(since = 2)]
    b: Vec<String>,
    #[pack_io(since = 3)]
    c: BTreeMap<u32, Option<i64>>,
}

#[test]
fn test_encoded_len_matches_for_derived_types() {
    let e = Evolving {
        a: 1 << 40,
        gone: String::from("not on the wire"),
        b: vec![String::from("x"); 3],
        c: BTreeMap::from([(300, Some(-1)), (1, None)]),
    };
    assert_eq!(e.encoded_len().unwrap(), encode(&e).unwrap().len());
    let big = vec![e.clone(); 200];
    assert_eq!(big.encoded_len().unwrap(), encode(&big).unwrap().len());
}

/// A hand-written impl must still be usable alongside derived ones.
struct Manual(u8);

impl pack_io::Serialize for Manual {
    fn serialize<E: Encode + ?Sized>(&self, e: &mut E) -> pack_io::Result<()> {
        e.write_byte(self.0)
    }
}

impl pack_io::Deserialize for Manual {
    fn deserialize<D: Decode + ?Sized>(d: &mut D) -> pack_io::Result<Self> {
        d.read_byte().map(Manual)
    }
}

#[derive(pack_io::Serialize, pack_io::Deserialize)]
#[pack_io(version = 1)]
struct HoldsManual {
    m: Manual,
}

#[test]
fn test_default_encoded_len_is_used_for_manual_impls() {
    let bytes = encode(&HoldsManual { m: Manual(9) }).unwrap();
    assert_eq!(bytes, [1, 1, 9]);
    assert_eq!(decode::<HoldsManual>(&bytes).unwrap().m.0, 9);
}
