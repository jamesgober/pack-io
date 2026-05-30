//! Use [`pack_io::Encoder::into_buffer`] to write many values into a single
//! caller-owned `Vec<u8>` — avoiding the per-call allocation that
//! [`pack_io::encode`] performs.
//!
//! Pattern: encode → send → take buffer back → repeat.
//!
//! Run with: `cargo run --example reuse_buffer --release`

use pack_io::{Decoder, Encoder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A single buffer reused across many encodes. Cap it at the largest
    // expected payload to avoid mid-loop reallocations.
    let mut buf: Vec<u8> = Vec::with_capacity(256);

    for round in 0..3 {
        // Move the buffer into the encoder, write a few values, take the
        // bytes back out. The Vec's capacity persists across rounds.
        let mut enc = Encoder::into_buffer(buf);
        enc.write(&(round as u64))?;
        enc.write(&format!("round-{round}"))?;
        enc.write(&true)?;
        buf = enc.into_inner();

        println!(
            "round {round} encoded into {} bytes (capacity {})",
            buf.len(),
            buf.capacity()
        );

        // Decode the three values back from the buffer to confirm shape.
        let mut dec = Decoder::new(&buf);
        let n: u64 = dec.read()?;
        let s: String = dec.read()?;
        let b: bool = dec.read()?;
        assert!(dec.is_empty());
        println!("  decoded: ({n}, {s:?}, {b})");

        buf.clear();
    }

    Ok(())
}
