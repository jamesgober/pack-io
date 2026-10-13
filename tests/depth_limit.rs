//! Recursion-depth limit (audit finding PACK-004).
//!
//! Before 2.0, about 5 KB of input for `struct Node { children: Vec<Node> }`
//! overflowed the stack and aborted the process. Every derived impl now
//! counts one nesting level against `Config::max_depth`, so the same input
//! returns `SerialError::DepthLimitExceeded` instead.

use std::io::Cursor;

use pack_io::{
    Config, Decode, Decoder, Deserialize, DeserializeView, IoDecoder, SerialError, Serialize,
    decode, decode_view, decode_with_config, encode,
};

#[derive(Debug, PartialEq, Serialize, Deserialize, DeserializeView)]
struct Node {
    children: Vec<Node>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Expr {
    Lit(u8),
    Neg(Vec<Expr>),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 1)]
struct VersionedTree {
    label: u8,
    kids: Vec<VersionedTree>,
}

#[derive(Debug, PartialEq, Serialize, DeserializeView)]
struct ViewTree<'a> {
    name: &'a str,
    kids: Vec<ViewTree<'a>>,
}

/// `depth` levels of `Node { children: [ … ] }`: each level is `varint(1)`,
/// the innermost is `varint(0)`.
fn node_chain(depth: usize) -> Vec<u8> {
    let mut bytes = vec![0x01; depth.saturating_sub(1)];
    bytes.push(0x00);
    bytes
}

fn build_node(depth: usize) -> Node {
    let mut node = Node { children: vec![] };
    for _ in 1..depth {
        node = Node {
            children: vec![node],
        };
    }
    node
}

#[test]
fn test_hostile_recursion_is_rejected_not_a_stack_overflow() {
    // 100,000 levels from 100 KB of input. 1.0 aborted the process here.
    let bytes = node_chain(100_000);
    let err = decode::<Node>(&bytes).unwrap_err();
    assert_eq!(
        err,
        SerialError::DepthLimitExceeded {
            max_depth: Config::DEFAULT_MAX_DEPTH
        }
    );
}

#[test]
fn test_hostile_enum_recursion_is_rejected() {
    let mut bytes = Vec::new();
    for _ in 0..50_000 {
        bytes.extend([1u8, 1]); // Neg([ ...
    }
    bytes.extend([0u8, 0]); // Lit(0)
    let err = decode::<Expr>(&bytes).unwrap_err();
    assert!(matches!(err, SerialError::DepthLimitExceeded { .. }));
}

#[test]
fn test_depth_exactly_at_the_limit_decodes() {
    let limit = Config::DEFAULT_MAX_DEPTH;
    let ok = decode::<Node>(&node_chain(limit)).unwrap();
    assert_eq!(ok, build_node(limit));
    let err = decode::<Node>(&node_chain(limit + 1)).unwrap_err();
    assert!(matches!(err, SerialError::DepthLimitExceeded { .. }));
}

#[test]
fn test_custom_limit_is_enforced_both_ways() {
    let tight = Config::new().with_max_depth(3);
    assert!(decode_with_config::<Node>(&node_chain(3), tight).is_ok());
    assert!(matches!(
        decode_with_config::<Node>(&node_chain(4), tight),
        Err(SerialError::DepthLimitExceeded { max_depth: 3 })
    ));

    let roomy = Config::new().with_max_depth(5_000);
    let deep = node_chain(1_000);
    assert!(decode::<Node>(&deep).is_err());
    // Raising the limit is the caller's opt-in for legitimately deep data.
    // Run on a big stack so the debug build has room for 1,000 frames.
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || decode_with_config::<Node>(&deep, roomy).unwrap())
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn test_zero_max_depth_is_an_invalid_config() {
    let err = Decoder::with_config(&[], Config::new().with_max_depth(0)).unwrap_err();
    assert!(matches!(err, SerialError::InvalidConfig { .. }));
}

#[test]
fn test_depth_is_restored_after_an_error() {
    let mut bytes = node_chain(10_000);
    let good = encode(&build_node(5)).unwrap();
    bytes.extend_from_slice(&good);
    let mut dec = Decoder::new(&good);
    // A failed decode must leave the decoder's depth at 0 again.
    let mut bad = Decoder::new(&bytes);
    assert!(bad.read::<Node>().is_err());
    assert_eq!(bad.depth(), 0);
    assert_eq!(dec.read::<Node>().unwrap(), build_node(5));
    assert_eq!(dec.depth(), 0);
}

#[test]
fn test_streaming_decoder_enforces_the_limit() {
    let mut dec = IoDecoder::new(Cursor::new(node_chain(50_000)));
    assert!(matches!(
        dec.read::<Node>(),
        Err(SerialError::DepthLimitExceeded { .. })
    ));
    let tight = Config::new().with_max_depth(2);
    let mut dec = IoDecoder::with_config(Cursor::new(node_chain(3)), tight).unwrap();
    assert!(dec.read::<Node>().is_err());
}

#[test]
fn test_view_decode_enforces_the_limit() {
    assert!(matches!(
        decode_view::<Node>(&node_chain(100_000)),
        Err(SerialError::DepthLimitExceeded { .. })
    ));
    // `ViewTree { name: "", kids: [ … ] }` per level.
    let mut bytes = Vec::new();
    for _ in 0..10_000 {
        bytes.extend([0u8, 1]);
    }
    bytes.extend([0u8, 0]);
    assert!(matches!(
        decode_view::<ViewTree<'_>>(&bytes),
        Err(SerialError::DepthLimitExceeded { .. })
    ));
}

#[test]
fn test_versioned_bodies_carry_the_depth_across_frames() {
    // Each level is a versioned frame wrapping the next. The depth must keep
    // counting inside each body; a fresh decoder per body would reset it.
    let tree = {
        let mut t = VersionedTree {
            label: 0,
            kids: vec![],
        };
        for i in 1..20u8 {
            t = VersionedTree {
                label: i,
                kids: vec![t],
            };
        }
        t
    };
    let bytes = encode(&tree).unwrap();
    assert_eq!(decode::<VersionedTree>(&bytes).unwrap(), tree);
    let tight = Config::new().with_max_depth(10);
    assert!(matches!(
        decode_with_config::<VersionedTree>(&bytes, tight),
        Err(SerialError::DepthLimitExceeded { max_depth: 10 })
    ));
}

#[test]
fn test_default_limit_fits_a_one_mib_stack_in_debug_builds() {
    // The documented default must be safe on the smallest common stack
    // (1 MiB, the Windows main thread) even without optimisations.
    let bytes = node_chain(Config::DEFAULT_MAX_DEPTH);
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || {
            let owned = decode::<Node>(&bytes).is_ok();
            let viewed = decode_view::<Node>(&bytes).is_ok();
            let streamed = IoDecoder::new(Cursor::new(&bytes[..]))
                .read::<Node>()
                .is_ok();
            owned && viewed && streamed
        })
        .unwrap();
    assert!(handle.join().unwrap());
}

/// A hand-written recursive type that follows the documented pattern.
struct Manual(Vec<Manual>);

impl Deserialize for Manual {
    fn deserialize<D: Decode + ?Sized>(dec: &mut D) -> pack_io::Result<Self> {
        dec.enter_nested()?;
        let children = Vec::<Manual>::deserialize(dec);
        dec.leave_nested();
        Ok(Manual(children?))
    }
}

#[test]
fn test_hand_written_impls_can_opt_in() {
    let err = decode::<Manual>(&node_chain(10_000)).err().unwrap();
    assert!(matches!(err, SerialError::DepthLimitExceeded { .. }));
    assert_eq!(decode::<Manual>(&node_chain(4)).unwrap().0.len(), 1);
}
