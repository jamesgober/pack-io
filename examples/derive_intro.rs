//! `#[derive(pack_io::Serialize, pack_io::Deserialize)]` on every kind of
//! struct and enum the derive supports — named-field structs, tuple
//! structs, unit structs, generics, and enums with every variant shape.
//!
//! Run with: `cargo run --example derive_intro --features derive --release`

use pack_io::{Deserialize, Serialize, decode, encode};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Account {
    id: u64,
    handle: String,
    flags: Vec<String>,
    active: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Coords(i32, i32, i32);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Marker;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Container<T> {
    label: String,
    value: T,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Event {
    Heartbeat,
    Login { user: u64, ip: String },
    Error(u32, String),
}

fn round_trip<T>(label: &str, value: T)
where
    T: Serialize + Deserialize + core::fmt::Debug + PartialEq,
{
    let bytes = encode(&value).expect("encode");
    let back: T = decode(&bytes).expect("decode");
    assert_eq!(back, value, "round-trip mismatch for {label}");
    println!("  {label:>32} → {:>3} bytes  ✓", bytes.len());
}

fn main() {
    println!("named struct");
    round_trip(
        "Account",
        Account {
            id: 42,
            handle: "jamesgober".into(),
            flags: vec!["admin".into(), "verified".into()],
            active: true,
        },
    );

    println!("\ntuple struct");
    round_trip("Coords", Coords(1, -2, 3));

    println!("\nunit struct");
    round_trip("Marker", Marker);

    println!("\ngeneric struct");
    round_trip(
        "Container<u64>",
        Container::<u64> {
            label: "seven".into(),
            value: 7,
        },
    );
    round_trip(
        "Container<Vec<u8>>",
        Container::<Vec<u8>> {
            label: "blob".into(),
            value: vec![0xde, 0xad, 0xbe, 0xef],
        },
    );

    println!("\nenum variants");
    round_trip("Event::Heartbeat", Event::Heartbeat);
    round_trip(
        "Event::Login",
        Event::Login {
            user: 100,
            ip: "10.0.0.1".into(),
        },
    );
    round_trip(
        "Event::Error",
        Event::Error(500, "internal server error".into()),
    );

    println!("\ndone — every derive variant round-tripped");
}
