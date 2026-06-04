//! End-to-end tests for the `#[derive(Serialize)]`, `#[derive(Deserialize)]`,
//! and `#[derive(DeserializeView)]` proc-macros.
//!
//! Covers the surface the macros are expected to handle: named-field structs,
//! tuple structs, unit structs, enums with every variant shape, generic
//! types, and the zero-copy `DeserializeView` lifetime path. The contract
//! these tests pin down: derive-generated code round-trips identically to
//! a hand-rolled impl.

use pack_io::{
    Decoder, Deserialize, DeserializeView, SerialError, Serialize, decode, decode_view, encode,
};

fn round_trip<T>(value: T)
where
    T: Serialize + Deserialize + PartialEq + core::fmt::Debug,
{
    let bytes = encode(&value).expect("encode");
    let back: T = decode(&bytes).expect("decode");
    assert_eq!(back, value);
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Named {
    id: u64,
    text: String,
    flag: bool,
}

#[test]
fn named_struct_round_trips() {
    round_trip(Named {
        id: 42,
        text: "hello".into(),
        flag: true,
    });
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Tuple(u32, String, Vec<u8>);

#[test]
fn tuple_struct_round_trips() {
    round_trip(Tuple(7, "world".into(), vec![1, 2, 3]));
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Unit;

#[test]
fn unit_struct_round_trips() {
    round_trip(Unit);
    let bytes = encode(&Unit).unwrap();
    assert!(bytes.is_empty());
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Nested {
    outer: u64,
    inner: Named,
    list: Vec<Tuple>,
}

#[test]
fn nested_struct_round_trips() {
    round_trip(Nested {
        outer: 1,
        inner: Named {
            id: 2,
            text: "nested".into(),
            flag: false,
        },
        list: vec![
            Tuple(10, "a".into(), vec![]),
            Tuple(20, "b".into(), vec![0xff]),
        ],
    });
}

// ---------------------------------------------------------------------------
// Generic types
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Wrapper<T> {
    inner: T,
    label: String,
}

#[test]
fn generic_struct_round_trips() {
    round_trip(Wrapper {
        inner: 42_u64,
        label: "generic".into(),
    });
    round_trip(Wrapper {
        inner: vec![1u32, 2, 3],
        label: "with-vec".into(),
    });
}

// ---------------------------------------------------------------------------
// Enums — every variant shape
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Shape {
    Point,
    Line(i32, i32),
    Polygon {
        vertices: Vec<(i32, i32)>,
        closed: bool,
    },
}

#[test]
fn enum_unit_variant_round_trips() {
    round_trip(Shape::Point);
}

#[test]
fn enum_tuple_variant_round_trips() {
    round_trip(Shape::Line(3, -7));
}

#[test]
fn enum_named_variant_round_trips() {
    round_trip(Shape::Polygon {
        vertices: vec![(0, 0), (1, 0), (1, 1), (0, 1)],
        closed: true,
    });
}

#[test]
fn enum_unknown_variant_index_is_rejected() {
    // Encode a Line, then patch the varint variant index to one beyond the
    // last legal variant (Shape has 3 variants: 0, 1, 2 → patch to 5).
    let mut bytes = encode(&Shape::Line(1, 2)).unwrap();
    bytes[0] = 5;
    let err = decode::<Shape>(&bytes).expect_err("unknown variant rejected");
    assert!(matches!(
        err,
        SerialError::UnknownVariant {
            kind: "Shape",
            index: 5
        }
    ));
}

// ---------------------------------------------------------------------------
// DeserializeView — borrowed fields
// ---------------------------------------------------------------------------

// Source type — used to produce the bytes the view borrows from.
#[derive(Serialize)]
struct OwnedMsg {
    id: u64,
    text: String,
    payload: Vec<u8>,
}

#[derive(Debug, PartialEq, DeserializeView)]
struct ViewMsg<'a> {
    id: u64,
    text: &'a str,
    payload: &'a [u8],
}

#[test]
fn deserialize_view_borrows_str_and_bytes() {
    let bytes = encode(&OwnedMsg {
        id: 7,
        text: "borrowed".into(),
        payload: vec![10, 20, 30],
    })
    .unwrap();

    let view: ViewMsg<'_> = decode_view(&bytes).unwrap();
    assert_eq!(view.id, 7);
    assert_eq!(view.text, "borrowed");
    assert_eq!(view.payload, &[10, 20, 30]);

    // Borrow checker: the view must not outlive the buffer.
    // (This is a compile-time guarantee; the runtime test just exercises it.)
}

#[derive(Debug, PartialEq, DeserializeView)]
struct ViewWithOptional<'a> {
    label: &'a str,
    extra: Option<&'a str>,
    count: u32,
}

#[derive(Serialize)]
struct OwnedWithOptional {
    label: String,
    extra: Option<String>,
    count: u32,
}

#[test]
fn deserialize_view_handles_option_of_borrowed() {
    let bytes = encode(&OwnedWithOptional {
        label: "hdr".into(),
        extra: Some("present".into()),
        count: 9,
    })
    .unwrap();
    let view: ViewWithOptional<'_> = decode_view(&bytes).unwrap();
    assert_eq!(view.label, "hdr");
    assert_eq!(view.extra, Some("present"));
    assert_eq!(view.count, 9);

    let bytes = encode(&OwnedWithOptional {
        label: "hdr".into(),
        extra: None,
        count: 0,
    })
    .unwrap();
    let view: ViewWithOptional<'_> = decode_view(&bytes).unwrap();
    assert_eq!(view.extra, None);
}

// ---------------------------------------------------------------------------
// Determinism — derive output matches hand-rolled output
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct DerivedTriple {
    a: u64,
    b: String,
    c: bool,
}

struct HandRolledTriple {
    a: u64,
    b: String,
    c: bool,
}

impl Serialize for HandRolledTriple {
    fn serialize<E: pack_io::Encode + ?Sized>(&self, enc: &mut E) -> pack_io::Result<()> {
        self.a.serialize(enc)?;
        self.b.serialize(enc)?;
        self.c.serialize(enc)
    }
}

#[test]
fn derived_serialize_produces_same_bytes_as_hand_rolled() {
    let derived = DerivedTriple {
        a: 100,
        b: "match".into(),
        c: true,
    };
    let hand = HandRolledTriple {
        a: 100,
        b: "match".into(),
        c: true,
    };
    assert_eq!(encode(&derived).unwrap(), encode(&hand).unwrap());
}

// ---------------------------------------------------------------------------
// View matches owning decode byte-for-byte
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct OwnedSimple {
    a: u64,
    b: String,
}

#[derive(Debug, PartialEq, DeserializeView)]
struct ViewSimple<'a> {
    a: u64,
    b: &'a str,
}

#[test]
fn view_and_owning_decode_agree() {
    let owned = OwnedSimple {
        a: 99,
        b: "string".into(),
    };
    let bytes = encode(&owned).unwrap();

    let back_owned: OwnedSimple = decode(&bytes).unwrap();
    let back_view: ViewSimple<'_> = decode_view(&bytes).unwrap();

    assert_eq!(back_owned.a, back_view.a);
    assert_eq!(back_owned.b.as_str(), back_view.b);
}

// ---------------------------------------------------------------------------
// Manual Decoder use through the borrowed-method seam
// ---------------------------------------------------------------------------

#[test]
fn decoder_read_length_prefixed_borrowed_zero_copies() {
    let bytes = encode(&"manual borrow").unwrap();
    let mut dec = Decoder::new(&bytes);
    let slice = dec.read_length_prefixed_borrowed().unwrap();
    let text = core::str::from_utf8(slice).unwrap();
    assert_eq!(text, "manual borrow");
}
