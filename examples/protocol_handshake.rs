//! Versioned protocol handshake — the pattern every cross-revision client
//! and server uses to negotiate capabilities. Two versions of the same
//! handshake message are defined locally; the program plays out every
//! combination of `v1 → v1`, `v1 → v2`, `v2 → v1`, `v2 → v2`, showing what
//! the receiver sees in each case and that no information is silently lost.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example protocol_handshake --features schema --release
//! ```

use pack_io::{Deserialize, Serialize, decode, encode, peek_version};

#[derive(Debug, Serialize, Deserialize)]
#[pack_io(version = 1)]
struct HandshakeV1 {
    client_id: u64,
    capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[pack_io(version = 2)]
struct HandshakeV2 {
    client_id: u64,
    capabilities: Vec<String>,
    /// Added in v2 — old encoders never wrote it; new decoders default
    /// it to `None`.
    #[pack_io(since = 2)]
    region: Option<String>,
}

fn header(label: &str) {
    println!("\n{}", "─".repeat(60));
    println!("{label}");
    println!("{}", "─".repeat(60));
}

fn main() {
    // ---------------------------------------------------------------
    // Two clients, two servers, every combination.
    // ---------------------------------------------------------------
    let v1_client_bytes = encode(&HandshakeV1 {
        client_id: 1,
        capabilities: vec!["tls".into()],
    })
    .unwrap();

    let v2_client_bytes = encode(&HandshakeV2 {
        client_id: 2,
        capabilities: vec!["tls".into(), "h2".into()],
        region: Some("us-east-1".into()),
    })
    .unwrap();

    println!("encoded sizes");
    println!("  v1 client:  {} bytes", v1_client_bytes.len());
    println!("  v2 client:  {} bytes", v2_client_bytes.len());

    // ---------------------------------------------------------------
    // peek_version: the server uses it to dispatch before committing
    // to a specific target type.
    // ---------------------------------------------------------------
    header("peek_version (server-side dispatch)");
    println!(
        "  v1 client wire reports version {}",
        peek_version(&v1_client_bytes).unwrap()
    );
    println!(
        "  v2 client wire reports version {}",
        peek_version(&v2_client_bytes).unwrap()
    );

    // ---------------------------------------------------------------
    // v1 → v1, v2 → v2: same revision both sides.
    // ---------------------------------------------------------------
    header("same-revision handshake (control case)");
    let h1: HandshakeV1 = decode(&v1_client_bytes).unwrap();
    let h2: HandshakeV2 = decode(&v2_client_bytes).unwrap();
    println!("  v1 server reads v1 client: {h1:?}");
    println!("  v2 server reads v2 client: {h2:?}");

    // ---------------------------------------------------------------
    // v1 → v2: old client, new server. Server gets default for `region`.
    // ---------------------------------------------------------------
    header("v1 client → v2 server (forward compat)");
    let upgraded: HandshakeV2 = decode(&v1_client_bytes).unwrap();
    println!("  decoded as v2: {upgraded:?}");
    println!(
        "  region field defaulted to {:?} (v1 never wrote it)",
        upgraded.region
    );

    // ---------------------------------------------------------------
    // v2 → v1: new client, old server. Server reads what it knows; the
    // length-framed body skips the trailing `region` bytes cleanly.
    // ---------------------------------------------------------------
    header("v2 client → v1 server (backward compat)");
    let downgraded: HandshakeV1 = decode(&v2_client_bytes).unwrap();
    println!("  decoded as v1: {downgraded:?}");
    println!("  v2-only `region` field is invisible to v1 server (skipped via body length)");

    println!("\ndone — every cross-version combination succeeded");
}
