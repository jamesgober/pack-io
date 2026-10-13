//! Fuzz target: arbitrary bytes through the streaming `IoDecoder`.
//!
//! The streaming decoder cannot see how much input remains, so it must
//! grow buffers only as bytes arrive. Run under `-malloc_limit_mb`, a
//! hostile length prefix in a short input must not trip the limit.

#![no_main]

use std::collections::BTreeMap;
use std::io::Cursor;

use libfuzzer_sys::fuzz_target;
use pack_io::{Deserialize, IoDecoder, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[pack_io(version = 3)]
struct Message {
    id: u64,
    text: String,
    #[pack_io(since = 2)]
    blob: Vec<u8>,
    #[pack_io(since = 3)]
    meta: BTreeMap<u16, Vec<String>>,
}

fuzz_target!(|data: &[u8]| {
    let mut dec = IoDecoder::new(Cursor::new(data));
    // Read a sequence of mixed values until the stream fails or ends.
    for _ in 0..4 {
        if dec.read::<(String, Vec<u8>)>().is_err() {
            break;
        }
        if dec.read::<Message>().is_err() {
            break;
        }
    }
    let _ = pack_io::decode_from::<Message, _>(&mut Cursor::new(data));
});
