//! The smallest useful demonstration of the Tier-1 API: encode a value,
//! decode it back, confirm equality.
//!
//! Run with: `cargo run --example basic_roundtrip --release`

use pack_io::{decode, encode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A heterogeneous tuple — pack-io handles each field with the
    // appropriate primitive impl, then concatenates the results.
    let original: (u64, bool, String, Option<i32>) = (42, true, String::from("pack-io"), Some(-7));

    let bytes = encode(&original)?;
    println!(
        "encoded {} bytes for a tuple of (u64, bool, String, Option<i32>)",
        bytes.len()
    );
    println!("bytes: {bytes:?}");

    let decoded: (u64, bool, String, Option<i32>) = decode(&bytes)?;
    assert_eq!(decoded, original);
    println!("round-trip equality: ok");

    Ok(())
}
