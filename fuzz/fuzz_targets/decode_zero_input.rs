//! Fuzz target: collections of elements that occupy no input bytes.
//!
//! `Box<()>`, `Rc<()>` and a struct whose fields are all `skip` encode to
//! nothing, so their element counts are not backed by input (re-audit
//! finding PACK-R01). Their memory is charged against `max_alloc` across
//! the whole decode: with a 64 KiB budget no input may push the decoder past
//! a small multiple of it (far below the 64 MiB malloc limit CI runs with),
//! however the counts are nested, split across versioned bodies, or routed
//! through views, streams or serde. The small budget keeps executions fast.

#![no_main]

use std::collections::{BTreeMap, HashSet};
use std::io::Cursor;
use std::rc::Rc;

use libfuzzer_sys::fuzz_target;
use pack_io::{Config, Deserialize, DeserializeView, IoDecoder, Serialize};

#[derive(Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, DeserializeView)]
struct OnlySkip {
    #[pack_io(skip)]
    a: u64,
    #[pack_io(skip)]
    b: u64,
}

#[derive(Debug, Serialize, Deserialize, DeserializeView)]
#[pack_io(version = 1)]
struct Holder {
    free: Vec<Box<()>>,
    skips: Vec<OnlySkip>,
}

fuzz_target!(|data: &[u8]| {
    let cfg = Config::new().with_max_alloc(1 << 16);
    let _ = pack_io::decode_with_config::<Vec<Box<()>>>(data, cfg);
    let _ = pack_io::decode_with_config::<Vec<Rc<()>>>(data, cfg);
    let _ = pack_io::decode_with_config::<Vec<Vec<OnlySkip>>>(data, cfg);
    let _ = pack_io::decode_with_config::<Vec<Holder>>(data, cfg);
    let _ = pack_io::decode_view_with_config::<Vec<Holder>>(data, cfg);
    let _ = pack_io::decode_with_config::<(BTreeMap<(), ()>, HashSet<Box<()>>)>(data, cfg);
    let _ = IoDecoder::with_config(Cursor::new(data), cfg)
        .and_then(|mut dec| dec.read::<Vec<Vec<Box<()>>>>());
    let _ = pack_io::serde::from_slice_with_config::<Vec<Vec<Box<()>>>>(data, cfg);
});
