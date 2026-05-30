//! Stream-encode a sequence of messages into a file, then stream-decode them
//! back. Demonstrates the `IoEncoder<W>` / `IoDecoder<R>` pair and the
//! `encode_into` / `decode_from` convenience helpers.
//!
//! Run with: `cargo run --example streaming_io --release`

use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor};

use pack_io::{IoDecoder, IoEncoder, decode_from, encode_into};

#[derive(Debug, PartialEq)]
struct Event {
    seq: u64,
    name: String,
    payload: Vec<u8>,
}

impl pack_io::Serialize for Event {
    fn serialize<E: pack_io::Encode + ?Sized>(&self, enc: &mut E) -> pack_io::Result<()> {
        self.seq.serialize(enc)?;
        self.name.serialize(enc)?;
        self.payload.serialize(enc)
    }
}

impl pack_io::Deserialize for Event {
    fn deserialize<D: pack_io::Decode + ?Sized>(dec: &mut D) -> pack_io::Result<Self> {
        Ok(Event {
            seq: u64::deserialize(dec)?,
            name: String::deserialize(dec)?,
            payload: Vec::<u8>::deserialize(dec)?,
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join("pack-io-streaming-example.pack");
    println!("temp file: {}", path.display());

    let events = vec![
        Event {
            seq: 1,
            name: String::from("connect"),
            payload: b"client=alice".to_vec(),
        },
        Event {
            seq: 2,
            name: String::from("publish"),
            payload: b"topic=alerts,size=128".to_vec(),
        },
        Event {
            seq: 3,
            name: String::from("disconnect"),
            payload: b"reason=idle".to_vec(),
        },
    ];

    // -----------------------------------------------------------------
    // 1) Stream-write three events into a file.
    // -----------------------------------------------------------------
    println!("\nwriting {} events with IoEncoder…", events.len());
    {
        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);
        let mut enc = IoEncoder::new(&mut writer);
        for event in &events {
            enc.write(event)?;
        }
    }
    let written = std::fs::metadata(&path)?.len();
    println!("wrote {written} bytes to disk");

    // -----------------------------------------------------------------
    // 2) Stream-read them back from the same file.
    // -----------------------------------------------------------------
    println!("\nreading them back with IoDecoder…");
    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    let mut dec = IoDecoder::new(reader);
    for expected in &events {
        let event: Event = dec.read()?;
        println!("  {:?}", event);
        assert_eq!(&event, expected);
    }

    // -----------------------------------------------------------------
    // 3) Single-shot encode_into / decode_from through a Cursor.
    // -----------------------------------------------------------------
    println!("\nencode_into / decode_from round-trip through a Cursor…");
    let mut buf: Vec<u8> = Vec::new();
    encode_into(&("hello", 42u64), &mut buf)?;
    let back: (String, u64) = decode_from(&mut Cursor::new(buf))?;
    println!("  decoded {:?}", back);
    assert_eq!(back, (String::from("hello"), 42));

    // Cleanup
    std::fs::remove_file(&path)?;
    println!("\ndone — streamed encode/decode round-trip succeeded");
    Ok(())
}
