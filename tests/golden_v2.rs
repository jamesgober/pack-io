//! Golden vectors for encodings introduced in pack-io 2.0.
//!
//! `tests/golden.rs` pins everything 1.0.1 could encode against files
//! written by 1.0.1. This file does the same for the types and derive
//! attributes added in 2.0 (spec 1.3, `docs/WIRE_FORMAT.md` §2.6 and §3.9),
//! against files under `tests/golden/v2/` written by pack-io 2.0.0, so any
//! later `2.x` release is checked against a real earlier one.
//!
//! The files are never regenerated from a later tree. Setting
//! `PACK_IO_WRITE_GOLDEN_V2=1` writes them, and is only meant to be used
//! from the release that defines them.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::num::{NonZeroI64, NonZeroU32};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use pack_io::{Deserialize, Serialize, decode, encode};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct List {
    value: u8,
    next: Option<Box<List>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 3)]
struct Retired {
    id: u64,
    #[pack_io(retired = 3)]
    legacy: String,
    #[pack_io(since = 2)]
    handle: String,
    #[pack_io(since = 3, default = "default_score")]
    score: i32,
}

fn default_score() -> i32 {
    -1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Skipping {
    id: u32,
    #[pack_io(skip)]
    cache: Vec<u8>,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Indexed {
    #[pack_io(index = 10)]
    Start,
    Stop {
        code: u8,
    },
    #[pack_io(index = 2)]
    Pause(u32),
}

fn vector<T>(name: &str, value: T)
where
    T: Serialize + Deserialize + PartialEq + Debug,
{
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "golden",
        "v2",
        &format!("{name}.bin"),
    ]
    .iter()
    .collect();
    let encoded = encode(&value).expect("encode");
    if std::env::var_os("PACK_IO_WRITE_GOLDEN_V2").is_some() {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, &encoded).expect("write golden");
        return;
    }
    let golden = std::fs::read(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(encoded, golden, "{name}: encoder output drifted from 2.0.0");
    let back: T = decode(&golden).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(
        back, value,
        "{name}: 2.0.0 bytes decode to a different value"
    );
}

#[test]
fn golden_v2_std_types() {
    vector("char", '\u{1f980}');
    vector("duration", Duration::new(3_600, 999_999_999));
    vector(
        "non_zero",
        (NonZeroU32::new(300).unwrap(), NonZeroI64::new(-5).unwrap()),
    );
    vector("phantom", PhantomData::<String>);
    vector("vec_deque", VecDeque::from(vec![1u16, 300, 7]));
    vector("box", Box::new(String::from("boxed")));
    vector("box_str", Box::<str>::from("text"));
    vector("box_slice", Box::<[u32]>::from(vec![1, 2, 3]));
    vector("rc_arc", (Rc::new(5u64), Arc::<str>::from("shared")));
    vector("cow", Cow::<str>::Owned(String::from("cow")));
}

#[test]
fn golden_v2_derive_attributes() {
    vector(
        "boxed_list",
        List {
            value: 1,
            next: Some(Box::new(List {
                value: 2,
                next: None,
            })),
        },
    );
    vector(
        "retired",
        Retired {
            id: 9,
            legacy: String::new(),
            handle: String::from("bob"),
            score: 70,
        },
    );
    vector(
        "skip",
        Skipping {
            id: 4,
            cache: Vec::new(),
            name: String::from("n"),
        },
    );
    vector("index_start", Indexed::Start);
    vector("index_stop", Indexed::Stop { code: 3 });
    vector("index_pause", Indexed::Pause(1_000));
}
