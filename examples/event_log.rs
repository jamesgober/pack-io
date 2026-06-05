//! Append-only event log — the pattern every WAL, audit log, message
//! queue, or replay buffer uses. Multiple event types (heterogeneous via
//! a single `Event` enum) are written to a tempfile through `IoEncoder`
//! and read back via `IoDecoder` in order. Demonstrates streaming I/O
//! over `std::fs::File` and the enum wire format with mixed variant
//! shapes.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example event_log --features derive --release
//! ```

use std::fs::File;
use std::io::{BufReader, BufWriter};

use pack_io::{Deserialize, IoDecoder, IoEncoder, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Event {
    /// New peer joined.
    Connected { peer: String, ts: u64 },
    /// Peer left.
    Disconnected {
        peer: String,
        ts: u64,
        reason: String,
    },
    /// Peer-to-peer message body.
    Message {
        from: String,
        to: String,
        body: Vec<u8>,
    },
    /// Periodic liveness ping.
    Heartbeat { ts: u64 },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join("pack-io-event-log.bin");

    let session = vec![
        Event::Connected {
            peer: "10.0.0.5".into(),
            ts: 1_000_000,
        },
        Event::Heartbeat { ts: 1_000_001 },
        Event::Message {
            from: "10.0.0.5".into(),
            to: "10.0.0.10".into(),
            body: b"hello".to_vec(),
        },
        Event::Heartbeat { ts: 1_000_002 },
        Event::Message {
            from: "10.0.0.10".into(),
            to: "10.0.0.5".into(),
            body: b"world".to_vec(),
        },
        Event::Heartbeat { ts: 1_000_003 },
        Event::Disconnected {
            peer: "10.0.0.5".into(),
            ts: 1_000_004,
            reason: "idle timeout".into(),
        },
    ];

    println!("writing {} events to {}", session.len(), path.display());

    // Stream-write — every event flows straight from `enc.write(&ev)` to
    // the file via the BufWriter, no intermediate Vec<u8>.
    {
        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);
        let mut enc = IoEncoder::new(&mut writer);
        for event in &session {
            enc.write(event)?;
        }
    }

    let on_disk = std::fs::metadata(&path)?.len();
    println!("wrote {on_disk} bytes\n");

    // Stream-read — replay the entire session, dispatching per variant.
    println!("replaying the log:");
    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    let mut dec = IoDecoder::new(reader);

    let mut replayed: Vec<Event> = Vec::new();
    for _ in 0..session.len() {
        let event: Event = dec.read()?;
        match &event {
            Event::Connected { peer, ts } => {
                println!("  [{ts}] CONNECT     {peer}");
            }
            Event::Disconnected { peer, ts, reason } => {
                println!("  [{ts}] DISCONNECT  {peer:<10} ({reason})");
            }
            Event::Message { from, to, body } => {
                println!("  [..]  MESSAGE     {from} → {to}  ({} bytes)", body.len());
            }
            Event::Heartbeat { ts } => {
                println!("  [{ts}] HEARTBEAT");
            }
        }
        replayed.push(event);
    }

    assert_eq!(replayed, session, "replay must match session exactly");
    std::fs::remove_file(&path)?;
    println!("\ndone — event log round-tripped through the file system");
    Ok(())
}
