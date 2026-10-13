# pack-io-fuzz

`cargo-fuzz` harness for pack-io.

Deliberately excluded from the parent Cargo workspace so stable / MSRV
builds of pack-io never compile this crate — `libfuzzer-sys` requires
nightly plus the AddressSanitizer instrumentation flags `cargo-fuzz`
injects at build time.

## Running locally

Install once:

```bash
cargo install cargo-fuzz
rustup install nightly
```

Then from this directory. Pass `seeds/<target>` as a second (read-only)
corpus directory so libFuzzer starts from valid encodings, and keep the
allocation limit tight: a hostile length prefix that makes the decoder
allocate more than the input can back trips `-malloc_limit_mb` and is
reported as a crash.

```bash
# Smoke run — one target for 60 seconds:
cargo +nightly fuzz run decode_string corpus/decode_string seeds/decode_string --     -max_total_time=60 -malloc_limit_mb=64

# Every target, 60 s each (what CI runs on Linux):
for t in $(cargo +nightly fuzz list); do
    mkdir -p "corpus/$t"
    cargo +nightly fuzz run "$t" "corpus/$t" "seeds/$t" --         -max_total_time=60 -timeout=10 -malloc_limit_mb=64 || exit 1
done
```

libFuzzer on Windows MSVC is not supported by this harness; on Windows,
run it under WSL. Crash inputs land in `fuzz/artifacts/<target>/` and can
be replayed:

```bash
cargo +nightly fuzz run decode_string fuzz/artifacts/decode_string/crash-<hash>
```

## Targets

| Target                       | What it hardens                                                |
|------------------------------|----------------------------------------------------------------|
| `decode_string`              | varint length + UTF-8 validation                               |
| `decode_vec_u8`              | byte-run fast path (`u8::deserialize_many`), input-bounded allocation |
| `decode_tuple`               | mixed primitive + length-prefixed shape                        |
| `decode_collection`          | `HashMap<String, Vec<u8>>` — count cap + per-entry decode      |
| `decode_view_str`            | zero-copy `&str` decode + lifetime / UTF-8 validation          |
| `decode_view_bytes`          | zero-copy `&[u8]` decode (no UTF-8 check path)                 |
| `decode_view_collection`     | `Vec<&str>` — collection of borrows                            |
| `decode_struct_derive`       | derive-generated struct deserialiser                           |
| `decode_enum_derive`         | derive-generated enum deserialiser + variant-index varint      |
| `decode_versioned`           | schema-evolution frame: version 0, body-length cap, borrowed body |
| `decode_btreemap`            | `BTreeMap<u64, String>` — ordered map non-std path             |
| `decode_btreeset`            | `BTreeSet<String>` — ordered set + per-element UTF-8 validation|
| `decode_hashset`             | `HashSet<u32>` — hash set preallocation cap                    |
| `decode_recursive`           | recursive derived structs / enums / versioned frames: `max_depth`, no stack overflow |
| `decode_io`                  | `IoDecoder` and `decode_from`: buffers grow only as bytes arrive |
| `decode_view_derive`         | view derive on versioned structs, enums and lifetime-less types; agrees with the owning decoder |
| `roundtrip`                  | decode → encode → decode, `encoded_len`, canonical-mode injectivity, owned vs view agreement |
| `decode_serde`               | the `serde` adapter: limits, round-trip, agreement with the native derive |

## Seeds

`seeds/<target>/` holds a few valid encodings of each target's types,
written with pack-io 1.1. They are inputs, not expectations; libFuzzer
grows its own working corpus in `corpus/` (git-ignored) from them.

## Contract

Every target asserts the same thing on every input: the decoder must
not panic, must not read past the input slice, must not allocate more
than the input can back (enforced by `-malloc_limit_mb=64`), and must
not overflow the stack on nested input. The differential targets also
assert that independent decode paths agree. Any failure is a bug in
pack-io and should be filed with the offending input attached.

The CI workflow runs every target for 60 seconds on every push as a
smoke check. Before the 1.1 release every target ran for 120 seconds
(about 514 million executions in total) with no findings.
