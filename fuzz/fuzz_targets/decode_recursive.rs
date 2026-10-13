//! Fuzz target: arbitrary bytes → recursive derived types.
//!
//! Hostile nesting must end in `SerialError::DepthLimitExceeded`, never in
//! a stack overflow (which would abort the fuzzer). Covers a recursive
//! struct, a recursive enum, a recursive versioned struct (whose depth must
//! carry across body frames), and the zero-copy view of the same struct.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pack_io::{Deserialize, DeserializeView, Serialize};

#[derive(Debug, Serialize, Deserialize, DeserializeView)]
struct Node {
    tag: u8,
    children: Vec<Node>,
}

#[derive(Debug, Serialize, Deserialize)]
enum Expr {
    Lit(i64),
    Neg(Vec<Expr>),
    Pair(Option<Vec<Expr>>, Vec<Expr>),
}

#[derive(Debug, Serialize, Deserialize)]
#[pack_io(version = 2)]
struct Tree {
    label: String,
    #[pack_io(since = 2)]
    kids: Vec<Tree>,
}

fuzz_target!(|data: &[u8]| {
    let _ = pack_io::decode::<Node>(data);
    let _ = pack_io::decode_view::<Node>(data);
    let _ = pack_io::decode::<Expr>(data);
    let _ = pack_io::decode::<Tree>(data);
});
