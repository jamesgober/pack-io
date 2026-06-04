//! Zero-copy decoding with `#[derive(pack_io::DeserializeView)]`.
//!
//! The `OwnedRecord` is the producer side — owned `String` / `Vec<u8>`
//! fields, encoded with the Tier-1 `encode` function. The `ViewRecord<'a>`
//! is the consumer side — `&'a str` / `&'a [u8]` fields that borrow
//! directly out of the input buffer. No per-field allocation; the bytes
//! the view points at are the same bytes we just wrote.
//!
//! Run with: `cargo run --example view_zero_copy --features derive --release`

use pack_io::{Deserialize, DeserializeView, Serialize, decode, decode_view, encode};

#[derive(Serialize, Deserialize)]
struct OwnedRecord {
    id: u64,
    name: String,
    payload: Vec<u8>,
    tags: Vec<String>,
}

#[derive(DeserializeView)]
struct ViewRecord<'a> {
    id: u64,
    name: &'a str,
    payload: &'a [u8],
    tags: Vec<&'a str>,
}

fn main() {
    let owned = OwnedRecord {
        id: 7,
        name: "system.alert.42".into(),
        payload: vec![0xab; 128],
        tags: vec!["critical".into(), "monitor".into(), "us-east-1".into()],
    };

    let bytes = encode(&owned).expect("encode");
    println!("encoded {} bytes", bytes.len());

    // Owning decode — allocates a fresh String, fresh Vec<u8>, fresh
    // Vec<String>. Each String inside `tags` is its own allocation.
    let back: OwnedRecord = decode(&bytes).expect("owning decode");
    println!(
        "\nowning decode:    id={} name={:?} payload={}B tags={:?}",
        back.id,
        back.name,
        back.payload.len(),
        back.tags
    );

    // Zero-copy decode — the &str and &[u8] fields point straight into
    // the `bytes` buffer above. The `tags` Vec is allocated (the
    // container has to live somewhere), but each &str inside it borrows.
    let view: ViewRecord<'_> = decode_view(&bytes).expect("view decode");
    println!(
        "\nzero-copy decode: id={} name={:?} payload={}B tags={:?}",
        view.id,
        view.name,
        view.payload.len(),
        view.tags
    );

    // Sanity-check that the view's bytes really are the same bytes as the
    // source buffer — not a copy.
    let name_ptr = view.name.as_ptr() as usize;
    let buf_start = bytes.as_ptr() as usize;
    let buf_end = buf_start + bytes.len();
    assert!(name_ptr >= buf_start && name_ptr < buf_end);
    println!(
        "\nview.name points inside the source buffer at offset {} ✓",
        name_ptr - buf_start
    );

    // The borrow checker enforces that `view` cannot outlive `bytes`.
    // Uncomment the next two lines to see the compile error.
    //
    //   drop(bytes);
    //   println!("{}", view.name); // borrow checker: error[E0505]
}
