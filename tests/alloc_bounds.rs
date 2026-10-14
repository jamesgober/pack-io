//! Allocation-bound tests: hostile length prefixes must not be able to make
//! the decoder allocate memory the input cannot back.
//!
//! A counting global allocator records, **for the current thread only**,
//! the largest single allocation and the peak number of live bytes while a
//! decode runs. Each test then asserts an upper bound derived from the
//! input size or the configured cap — never from the attacker's declared
//! length. These are the regression tests for audit findings PACK-001
//! (in-memory `Vec<u8>`), PACK-002 (streaming decoder), PACK-003
//! (versioned bodies ignoring `Config`), PACK-011 (`decode_from`), and the
//! 2.0 re-audit findings PACK-R01 (elements that occupy no input bytes),
//! PACK-R09 (primitive arrays) and PACK-R10 (stream pre-sizing).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io::{Cursor, Read};
use std::rc::Rc;
use std::sync::Arc;

use pack_io::{
    Config, Decoder, Deserialize, DeserializeView, IoDecoder, SerialError, Serialize, decode,
    decode_from_with_config, decode_view_with_config, decode_with_config, encode,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Thread-local counting allocator
// ---------------------------------------------------------------------------

struct Tracking;

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static PEAK: Cell<usize> = const { Cell::new(0) };
    static LARGEST: Cell<usize> = const { Cell::new(0) };
}

fn record_alloc(size: usize) {
    let _ = ACTIVE.try_with(|active| {
        if active.get() {
            let live = LIVE.with(|l| {
                let v = l.get().saturating_add(size);
                l.set(v);
                v
            });
            PEAK.with(|p| p.set(p.get().max(live)));
            LARGEST.with(|m| m.set(m.get().max(size)));
        }
    });
}

fn record_dealloc(size: usize) {
    let _ = ACTIVE.try_with(|active| {
        if active.get() {
            LIVE.with(|l| l.set(l.get().saturating_sub(size)));
        }
    });
}

// SAFETY: every method forwards to `System` with the caller's arguments
// unchanged, so the `GlobalAlloc` contract is upheld by `System`. The only
// extra work is updating thread-local `Cell<usize>` counters, which never
// allocates (const-initialised thread locals).
unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_alloc(layout.size());
        // SAFETY: forwarded verbatim; the caller upholds `alloc`'s contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_alloc(layout.size());
        // SAFETY: forwarded verbatim; the caller upholds the contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record_dealloc(layout.size());
        // SAFETY: `ptr` / `layout` come from a matching `alloc` call.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_dealloc(layout.size());
        record_alloc(new_size);
        // SAFETY: forwarded verbatim; the caller upholds `realloc`'s contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Tracking = Tracking;

/// What a closure allocated on this thread.
#[derive(Debug, Clone, Copy)]
struct Usage {
    /// Peak bytes live at once (net of frees) during the closure.
    peak: usize,
    /// Largest single allocation request.
    largest: usize,
}

fn measure<R>(f: impl FnOnce() -> R) -> (R, Usage) {
    LIVE.with(|l| l.set(0));
    PEAK.with(|p| p.set(0));
    LARGEST.with(|m| m.set(0));
    ACTIVE.with(|a| a.set(true));
    let out = f();
    ACTIVE.with(|a| a.set(false));
    let usage = Usage {
        peak: PEAK.with(Cell::get),
        largest: LARGEST.with(Cell::get),
    };
    (out, usage)
}

/// Assert `usage` stays under `limit` bytes, both for the largest single
/// allocation and for the live peak.
#[track_caller]
fn assert_bounded(usage: Usage, limit: usize) {
    assert!(
        usage.largest <= limit && usage.peak <= limit,
        "allocation exceeded {limit} bytes: {usage:?}"
    );
}

fn varint(mut n: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
    out
}

/// Small fixed slack for incidental allocations (error payloads, boxed
/// values) unrelated to the declared length.
const SLACK: usize = 4 * 1024;

/// The streaming decoder reads at most one chunk ahead of the data.
const STREAM_CHUNK: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// PACK-001 — in-memory decoder
// ---------------------------------------------------------------------------

#[test]
fn test_vec_u8_hostile_length_allocates_nothing() {
    let bytes = varint(1 << 30); // 5 bytes declaring 1 GiB
    let (res, usage) = measure(|| decode::<Vec<u8>>(&bytes));
    assert!(matches!(res, Err(SerialError::UnexpectedEof { .. })));
    assert_bounded(usage, SLACK);
}

#[test]
fn test_vec_u8_hostile_length_under_tight_cap_allocates_nothing() {
    let bytes = varint(1 << 20);
    let cfg = Config::new().with_max_alloc(1 << 20);
    let (res, usage) = measure(|| decode_with_config::<Vec<u8>>(&bytes, cfg));
    assert!(res.is_err());
    assert_bounded(usage, SLACK);
}

#[test]
fn test_string_and_collections_hostile_counts_are_bounded_by_input() {
    let bytes = varint(1 << 29);
    let (r1, u1) = measure(|| decode::<String>(&bytes));
    let (r2, u2) = measure(|| decode::<Vec<String>>(&bytes));
    let (r3, u3) = measure(|| decode::<HashMap<String, Vec<u64>>>(&bytes));
    let (r4, u4) = measure(|| decode::<HashSet<[u64; 8]>>(&bytes));
    let (r5, u5) = measure(|| decode::<BTreeMap<u64, String>>(&bytes));
    let (r6, u6) = measure(|| decode::<Vec<[u8; 32]>>(&bytes));
    for (res, usage) in [
        (r1.is_err(), u1),
        (r2.is_err(), u2),
        (r3.is_err(), u3),
        (r4.is_err(), u4),
        (r5.is_err(), u5),
        (r6.is_err(), u6),
    ] {
        assert!(res);
        assert_bounded(usage, SLACK);
    }
}

#[test]
fn test_nested_hostile_counts_stay_bounded() {
    // An outer Vec of 1000 inner Vecs, each declaring 2^28 elements but
    // carrying none: every inner count is refused before allocating.
    let mut bytes = varint(1000);
    for _ in 0..1000 {
        bytes.extend(varint(1 << 28));
    }
    let (res, usage) = measure(|| decode::<Vec<Vec<u64>>>(&bytes));
    assert!(res.is_err());
    assert_bounded(usage, 64 * bytes.len() + SLACK);
}

#[test]
fn test_array_u8_decode_does_not_touch_the_heap() {
    let bytes = encode(&[0xa5u8; 32]).unwrap();
    let (res, usage) = measure(|| decode::<[u8; 32]>(&bytes));
    assert_eq!(res.unwrap(), [0xa5; 32]);
    assert_eq!(usage.largest, 0, "{usage:?}");
}

// ---------------------------------------------------------------------------
// PACK-002 — streaming decoder
// ---------------------------------------------------------------------------

#[test]
fn test_io_decoder_hostile_string_length_reads_at_most_one_chunk() {
    let bytes = varint(1 << 30);
    let (res, usage) = measure(|| IoDecoder::new(Cursor::new(&bytes[..])).read::<String>());
    assert!(matches!(res, Err(SerialError::UnexpectedEof { .. })));
    assert_bounded(usage, STREAM_CHUNK + SLACK);
}

#[test]
fn test_io_decoder_hostile_vec_length_reads_at_most_one_chunk() {
    let bytes = varint(1 << 30);
    let (res, usage) = measure(|| IoDecoder::new(Cursor::new(&bytes[..])).read::<Vec<u8>>());
    assert!(res.is_err());
    assert_bounded(usage, STREAM_CHUNK + SLACK);
}

/// A reader that delivers `left` real bytes in small reads, then reports
/// end of stream.
struct Truncated {
    left: usize,
}

impl Read for Truncated {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf.len().min(self.left).min(4096);
        buf[..n].fill(b'x');
        self.left -= n;
        Ok(n)
    }
}

#[test]
fn test_io_decoder_allocation_tracks_bytes_actually_received() {
    // Declares 1 GiB, delivers 300 KiB, then EOF: memory must track the
    // 300 KiB actually received, not the 1 GiB declared.
    let received = 300 * 1024;
    let source = Cursor::new(varint(1 << 30)).chain(Truncated { left: received });
    let (res, usage) = measure(|| IoDecoder::new(source).read::<Vec<u8>>());
    assert!(res.is_err());
    // Doubling growth: at most ~2x what arrived, plus one chunk.
    assert_bounded(usage, 2 * received + 2 * STREAM_CHUNK + SLACK);
}

#[test]
fn test_io_decoder_honours_tight_max_alloc() {
    let bytes = varint(1 << 20);
    let cfg = Config::new().with_max_alloc(1024);
    let (res, usage) = measure(|| {
        IoDecoder::with_config(Cursor::new(&bytes[..]), cfg)
            .unwrap()
            .read::<String>()
    });
    assert!(matches!(res, Err(SerialError::InvalidLength { .. })));
    assert_bounded(usage, SLACK);
}

// ---------------------------------------------------------------------------
// PACK-003 — versioned bodies
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[pack_io(version = 1)]
struct Versioned {
    blob: Vec<u8>,
}

#[test]
fn test_versioned_hostile_body_length_allocates_nothing() {
    let mut bytes = varint(1);
    bytes.extend(varint(1 << 30));
    let (res, usage) = measure(|| decode::<Versioned>(&bytes));
    assert!(matches!(res, Err(SerialError::InvalidLength { .. })));
    assert_bounded(usage, SLACK);
}

#[test]
fn test_versioned_body_inherits_callers_max_alloc() {
    // The body (3 bytes) declares an inner 1 MiB Vec<u8>. Under a 64-byte
    // cap the inner length must be refused exactly as it would be outside
    // a versioned frame.
    let inner = varint(1 << 20);
    let mut bytes = varint(1);
    bytes.extend(varint(inner.len() as u64));
    bytes.extend(&inner);
    let cfg = Config::new().with_max_alloc(64);
    let (res, usage) = measure(|| decode_with_config::<Versioned>(&bytes, cfg));
    assert!(
        matches!(res, Err(SerialError::InvalidLength { declared, .. }) if declared == 1 << 20),
        "{res:?}"
    );
    assert_bounded(usage, SLACK);
}

#[test]
fn test_versioned_body_through_io_decoder_is_bounded() {
    let mut bytes = varint(1);
    bytes.extend(varint(1 << 29));
    let (res, usage) = measure(|| IoDecoder::new(Cursor::new(&bytes[..])).read::<Versioned>());
    assert!(res.is_err());
    assert_bounded(usage, STREAM_CHUNK + SLACK);
}

#[test]
fn test_versioned_decode_makes_one_allocation_for_its_field() {
    let bytes = encode(&Versioned { blob: vec![7; 10] }).unwrap();
    let (res, usage) = measure(|| decode::<Versioned>(&bytes));
    assert_eq!(res.unwrap().blob, vec![7; 10]);
    // Only the 10-byte field: the body is borrowed, not copied.
    assert_eq!(usage.largest, 10, "{usage:?}");
}

// ---------------------------------------------------------------------------
// PACK-011 — decode_from
// ---------------------------------------------------------------------------

#[test]
fn test_decode_from_stops_reading_at_max_alloc() {
    let cfg = Config::new().with_max_alloc(4096);
    let mut endless = std::io::repeat(0u8);
    let (res, usage) = measure(|| decode_from_with_config::<u8, _>(&mut endless, cfg));
    assert!(matches!(
        res,
        Err(SerialError::InvalidLength { declared: 4097, .. })
    ));
    assert_bounded(usage, 4 * 4096 + SLACK);
}

// ---------------------------------------------------------------------------
// Property: no decode of random input allocates beyond a small multiple of
// the input size.
// ---------------------------------------------------------------------------

/// Generous per-input-byte ceiling: a legitimately decoded value can be
/// larger than its encoding (a `Vec<u64>` element is 1 wire byte but 8
/// bytes in memory; a hash-map entry carries table overhead), but never by
/// more than a constant factor, and never by the 1 GiB a declared length
/// could ask for.
fn input_bound(input: &[u8]) -> usize {
    256 * input.len() + 128 * 1024
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn prop_random_input_allocation_is_bounded_by_input(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
        let (_, u1) = measure(|| decode::<Vec<Vec<u8>>>(&bytes));
        let (_, u2) = measure(|| decode::<HashMap<u16, String>>(&bytes));
        let (_, u3) = measure(|| decode::<(String, Vec<u64>, Option<Vec<String>>)>(&bytes));
        let (_, u4) = measure(|| decode::<Versioned>(&bytes));
        let (_, u5) = measure(|| {
            let mut dec = Decoder::new(&bytes);
            dec.read::<BTreeMap<u8, Vec<u32>>>()
        });
        let limit = input_bound(&bytes);
        for u in [u1, u2, u3, u4, u5] {
            prop_assert!(u.peak <= limit && u.largest <= limit, "{:?} > {}", u, limit);
        }
    }

    #[test]
    fn prop_random_stream_allocation_is_bounded(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
        let (_, usage) = measure(|| IoDecoder::new(Cursor::new(&bytes[..])).read::<(Vec<u8>, String)>());
        prop_assert!(usage.peak <= STREAM_CHUNK * 2 + SLACK, "{:?}", usage);
    }
}

// ---------------------------------------------------------------------------
// Optional integrations share the same guarantees.
// ---------------------------------------------------------------------------

#[cfg(feature = "serde")]
#[test]
fn test_serde_adapter_hostile_lengths_are_bounded() {
    let bytes = varint(1 << 30);
    let (r1, u1) = measure(|| pack_io::serde::from_slice::<Vec<u8>>(&bytes));
    let (r2, u2) = measure(|| pack_io::serde::from_slice::<String>(&bytes));
    let (r3, u3) = measure(|| pack_io::serde::from_slice::<Vec<String>>(&bytes));
    let (r4, u4) = measure(|| {
        let mut dec = IoDecoder::new(Cursor::new(&bytes[..]));
        pack_io::serde::from_decode::<String, _>(&mut dec)
    });
    assert!(r1.is_err() && r2.is_err() && r3.is_err() && r4.is_err());
    for usage in [u1, u2, u3] {
        assert_bounded(usage, SLACK);
    }
    assert_bounded(u4, STREAM_CHUNK + SLACK);
}

#[cfg(feature = "bytes")]
#[test]
fn test_bytes_hostile_lengths_are_bounded() {
    let bytes = varint(1 << 30);
    let (r1, u1) = measure(|| decode::<bytes::Bytes>(&bytes));
    let (r2, u2) = measure(|| decode::<bytes::BytesMut>(&bytes));
    assert!(r1.is_err() && r2.is_err());
    assert_bounded(u1, SLACK);
    assert_bounded(u2, SLACK);
}

// ---------------------------------------------------------------------------
// PACK-R01 — collection elements that occupy no input bytes
// ---------------------------------------------------------------------------
//
// `Box<()>`, `Rc<()>`, `Arc<()>` and a struct whose fields are all `skip`
// encode to nothing, so their element count is not backed by input. Their
// memory is charged against `max_alloc` across the whole decode instead.

/// Encodes to zero bytes but is 16 bytes in memory.
#[derive(Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct OnlySkip {
    #[pack_io(skip)]
    a: u64,
    #[pack_io(skip)]
    b: u64,
}

/// A 1 MiB budget: every hostile count below stays within it (so the
/// per-collection count check does not fire), while the memory the count
/// asks for is many times larger.
const BUDGET: usize = 1 << 20;

fn budget() -> Config {
    Config::new().with_max_alloc(BUDGET)
}

/// Growing a `Vec` can briefly hold twice its length; an `Rc` / `Arc`
/// element also owns a 16-byte heap block the charge does not see.
const ZERO_INPUT_LIMIT: usize = 2 * BUDGET + SLACK;
const ZERO_INPUT_LIMIT_RC: usize = 4 * BUDGET + SLACK;

#[test]
fn test_zero_input_elements_are_charged_against_max_alloc() {
    // 3 bytes declaring 2^20 elements: 8 MiB of `Box<()>`, 16 MiB of
    // `OnlySkip`, 24 MiB of `Rc<()>` before the fix.
    let bytes = varint(1 << 20);
    let (r1, u1) = measure(|| decode_with_config::<Vec<Box<()>>>(&bytes, budget()).map(drop));
    let (r2, u2) = measure(|| decode_with_config::<Vec<OnlySkip>>(&bytes, budget()).map(drop));
    let (r3, u3) = measure(|| decode_with_config::<VecDeque<Box<()>>>(&bytes, budget()).map(drop));
    let (r4, u4) = measure(|| decode_with_config::<Box<[Box<()>]>>(&bytes, budget()).map(drop));
    for (res, usage) in [(r1, u1), (r2, u2), (r3, u3), (r4, u4)] {
        assert!(
            matches!(res, Err(SerialError::InvalidLength { .. })),
            "{res:?}"
        );
        assert_bounded(usage, ZERO_INPUT_LIMIT);
    }
    let (r5, u5) = measure(|| decode_with_config::<Vec<Rc<()>>>(&bytes, budget()).map(drop));
    let (r6, u6) = measure(|| decode_with_config::<Vec<Arc<()>>>(&bytes, budget()).map(drop));
    for (res, usage) in [(r5, u5), (r6, u6)] {
        assert!(
            matches!(res, Err(SerialError::InvalidLength { .. })),
            "{res:?}"
        );
        assert_bounded(usage, ZERO_INPUT_LIMIT_RC);
    }
}

#[test]
fn test_zero_input_budget_spans_the_whole_decode() {
    // 64 inner vectors of 2^17 `Box<()>` (1 MiB each): each fits the
    // budget alone, together they need 64 MiB. Before the fix every inner
    // vector was checked on its own and the decode allocated all of it.
    let mut bytes = varint(64);
    for _ in 0..64 {
        bytes.extend(varint(1 << 17));
    }
    let (res, usage) =
        measure(|| decode_with_config::<Vec<Vec<Box<()>>>>(&bytes, budget()).map(drop));
    assert!(
        matches!(res, Err(SerialError::InvalidLength { .. })),
        "{res:?}"
    );
    assert_bounded(usage, ZERO_INPUT_LIMIT + 64 * 24);
}

// `Box<()>` is the point: a non-zero-sized element that encodes to nothing.
#[allow(clippy::vec_box)]
#[derive(Debug, PartialEq, Serialize, Deserialize, DeserializeView)]
#[pack_io(version = 1)]
struct FreeHolder {
    free: Vec<Box<()>>,
}

#[test]
fn test_zero_input_budget_crosses_versioned_bodies() {
    // Each versioned body carries one 1 MiB `Vec<Box<()>>`; the budget must
    // carry over from body to body instead of restarting in each.
    let inner = varint(1 << 17);
    let mut bytes = varint(64);
    for _ in 0..64 {
        bytes.extend(varint(1));
        bytes.extend(varint(inner.len() as u64));
        bytes.extend(&inner);
    }
    let (r1, u1) = measure(|| decode_with_config::<Vec<FreeHolder>>(&bytes, budget()).map(drop));
    let (r2, u2) =
        measure(|| decode_view_with_config::<Vec<FreeHolder>>(&bytes, budget()).map(drop));
    for (res, usage) in [(r1, u1), (r2, u2)] {
        assert!(
            matches!(res, Err(SerialError::InvalidLength { .. })),
            "{res:?}"
        );
        assert_bounded(usage, ZERO_INPUT_LIMIT + 64 * 24);
    }
}

#[test]
fn test_zero_input_elements_through_io_decoder_and_views_are_charged() {
    let bytes = varint(1 << 20);
    let (r1, u1) = measure(|| {
        IoDecoder::with_config(Cursor::new(&bytes[..]), budget())
            .unwrap()
            .read::<Vec<Box<()>>>()
            .map(drop)
    });
    let (r2, u2) = measure(|| decode_view_with_config::<Vec<Box<()>>>(&bytes, budget()).map(drop));
    for (res, usage) in [(r1, u1), (r2, u2)] {
        assert!(
            matches!(res, Err(SerialError::InvalidLength { .. })),
            "{res:?}"
        );
        assert_bounded(usage, ZERO_INPUT_LIMIT);
    }
}

#[test]
fn test_zero_input_maps_and_sets_are_charged() {
    // Entries that consume nothing collapse into one key, so the memory is
    // small, but the loop would otherwise run 2^20 times for 3 bytes. A
    // zero-sized entry is charged one byte, so two such maps of 2^20 entries
    // exceed a 1 MiB budget together.
    let bytes = varint(1 << 20);
    let mut two = bytes.clone();
    two.extend(&bytes);
    let r1 = decode_with_config::<(BTreeMap<(), ()>, BTreeMap<(), ()>)>(&two, budget()).map(drop);
    let r2 = decode_with_config::<HashSet<Box<()>>>(&bytes, budget()).map(drop);
    let r3 = decode_with_config::<HashMap<OnlySkip, OnlySkip>>(&bytes, budget()).map(drop);
    for res in [r1, r2, r3] {
        assert!(
            matches!(res, Err(SerialError::InvalidLength { .. })),
            "{res:?}"
        );
    }
}

#[test]
fn test_legitimate_zero_input_collections_still_decode() {
    let boxes: Vec<Box<()>> = (0..1000).map(|_| Box::new(())).collect();
    let skips: Vec<OnlySkip> = (0..1000).map(|_| OnlySkip::default()).collect();
    let units = vec![(); 100_000];
    assert_eq!(encode(&boxes).unwrap().len(), 2);
    assert_eq!(
        decode::<Vec<Box<()>>>(&encode(&boxes).unwrap()).unwrap(),
        boxes
    );
    assert_eq!(
        decode::<Vec<OnlySkip>>(&encode(&skips).unwrap()).unwrap(),
        skips
    );
    assert_eq!(decode::<Vec<()>>(&encode(&units).unwrap()).unwrap(), units);
    // Exactly at the budget: 2^17 `Box<()>` is 1 MiB.
    let at_budget = varint(1 << 17);
    assert_eq!(
        decode_with_config::<Vec<Box<()>>>(&at_budget, budget())
            .unwrap()
            .len(),
        1 << 17
    );
    // Ordinary elements are unaffected by the budget: each consumes input.
    let big: Vec<u64> = (0..300_000).collect();
    assert_eq!(
        decode_with_config::<Vec<u64>>(&encode(&big).unwrap(), budget()).unwrap(),
        big
    );
}

#[cfg(feature = "serde")]
#[test]
fn test_zero_input_elements_through_serde_are_charged() {
    let bytes = varint(1 << 20);
    let (r1, u1) = measure(|| {
        pack_io::serde::from_slice_with_config::<Vec<Box<()>>>(&bytes, budget()).map(drop)
    });
    let (r2, u2) = measure(|| {
        let mut dec = IoDecoder::with_config(Cursor::new(&bytes[..]), budget()).unwrap();
        pack_io::serde::from_decode::<Vec<Box<()>>, _>(&mut dec).map(drop)
    });
    for (res, usage) in [(r1, u1), (r2, u2)] {
        assert!(
            matches!(res, Err(SerialError::InvalidLength { .. })),
            "{res:?}"
        );
        assert_bounded(usage, ZERO_INPUT_LIMIT);
    }
}

// ---------------------------------------------------------------------------
// PACK-R09 — primitive arrays decode on the stack
// ---------------------------------------------------------------------------

#[test]
fn test_primitive_arrays_decode_without_touching_the_heap() {
    let ints = encode(&[1u64, 300, u64::MAX, 0]).unwrap();
    let floats = encode(&[1.5f32, -2.0, 0.25]).unwrap();
    let mixed = encode(&([true, false], ['a', '\u{e9}'], [-7i32, 9])).unwrap();
    let (r1, u1) = measure(|| decode::<[u64; 4]>(&ints));
    let (r2, u2) = measure(|| decode::<[f32; 3]>(&floats));
    let (r3, u3) = measure(|| decode::<([bool; 2], [char; 2], [i32; 2])>(&mixed));
    assert_eq!(r1.unwrap(), [1, 300, u64::MAX, 0]);
    assert_eq!(r2.unwrap(), [1.5, -2.0, 0.25]);
    assert_eq!(r3.unwrap(), ([true, false], ['a', '\u{e9}'], [-7, 9]));
    for usage in [u1, u2, u3] {
        assert_eq!(usage.largest, 0, "{usage:?}");
    }
    // A short input still fails cleanly.
    assert!(decode::<[u64; 4]>(&ints[..2]).is_err());
}

// ---------------------------------------------------------------------------
// PACK-R10 — streams pre-size collections modestly
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct Node {
    children: Vec<Node>,
}

#[test]
fn test_io_decoder_nested_hostile_counts_pin_little_memory() {
    // 120 nested levels, each declaring 2^30 children, then the stream
    // ends. Every open level pre-sizes its vector before the first child
    // arrives: 64 KiB per level (7.5 MiB) before the fix, 4 KiB now.
    let mut bytes = Vec::new();
    for _ in 0..120 {
        bytes.extend(varint(1 << 30));
    }
    let (res, usage) = measure(|| IoDecoder::new(Cursor::new(&bytes[..])).read::<Node>());
    assert!(res.is_err());
    assert_bounded(usage, 120 * 4 * 1024 + SLACK);
}
