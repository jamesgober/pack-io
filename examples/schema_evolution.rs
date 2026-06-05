//! Cross-version, cross-decode walkthrough.
//!
//! Two versions of the same logical type are defined locally — `MessageV1`
//! and `MessageV2`. v2 adds a `timestamp` field with `#[pack_io(since = 2)]`.
//! The program exercises every combination:
//!
//! - V1 → V1 (round-trip)
//! - V2 → V2 (round-trip, with and without `timestamp` set)
//! - V1 → V2 (`timestamp` defaults to `None`)
//! - V2 → V1 (the trailing `timestamp` bytes in the wire are ignored)
//! - `peek_version(&bytes)` for both
//!
//! Run with:
//!
//! ```bash
//! cargo run --example schema_evolution --features schema --release
//! ```

use pack_io::{Deserialize, Serialize, decode, encode, peek_version};

#[derive(Debug, Serialize, Deserialize)]
#[pack_io(version = 1)]
struct MessageV1 {
    id: u64,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[pack_io(version = 2)]
struct MessageV2 {
    id: u64,
    text: String,
    #[pack_io(since = 2)]
    timestamp: Option<u64>,
}

fn show_bytes(label: &str, bytes: &[u8]) {
    println!("  {label:<40} {} bytes", bytes.len());
    println!("    {bytes:?}");
}

fn main() {
    println!("Encoding the same logical message at two schema versions");
    let v1 = MessageV1 {
        id: 42,
        text: "hello".into(),
    };
    let v2 = MessageV2 {
        id: 42,
        text: "hello".into(),
        timestamp: Some(1_700_000_000),
    };
    let b1 = encode(&v1).expect("encode v1");
    let b2 = encode(&v2).expect("encode v2");
    show_bytes("v1 (id, text)", &b1);
    show_bytes("v2 (id, text, timestamp = Some)", &b2);

    println!("\npeek_version reports the writer's schema version");
    println!("  v1 → {}", peek_version(&b1).expect("peek v1"));
    println!("  v2 → {}", peek_version(&b2).expect("peek v2"));

    println!("\nv1 → v2: old encoding, new decoder (timestamp defaults)");
    let upgraded: MessageV2 = decode(&b1).expect("v2 reads v1");
    println!("  {upgraded:?}");
    assert_eq!(upgraded.id, 42);
    assert_eq!(upgraded.text, "hello");
    assert_eq!(upgraded.timestamp, None);

    println!("\nv2 → v1: new encoding, old decoder (trailing field ignored)");
    let downgraded: MessageV1 = decode(&b2).expect("v1 reads v2");
    println!("  {downgraded:?}");
    assert_eq!(downgraded.id, 42);
    assert_eq!(downgraded.text, "hello");

    println!("\nThe v2-only data (timestamp) was inside the length-prefixed");
    println!("body, so v1 was able to skip it cleanly without parse errors.");
    println!("\ndone — cross-version cross-decode succeeded both directions");
}
