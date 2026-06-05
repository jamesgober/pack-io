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

Then from this directory (or via `cargo +nightly fuzz` from the parent):

```bash
# Smoke run — fuzz one target for 30 seconds, fail fast on the first
# crash:
cargo +nightly fuzz run decode_string -- -max_total_time=30

# Long run — 10 minutes:
cargo +nightly fuzz run decode_string -- -max_total_time=600

# All targets, 30s each:
for t in decode_string decode_vec_u8 decode_tuple decode_collection \
         decode_view_str decode_struct_derive decode_enum_derive \
         decode_versioned; do
    cargo +nightly fuzz run "$t" -- -max_total_time=30 || exit 1
done
```

Crash corpora land in `fuzz/artifacts/<target>/`; failing inputs are
saved with a hash filename and can be replayed:

```bash
cargo +nightly fuzz run decode_string fuzz/artifacts/decode_string/crash-<hash>
```

## Targets

| Target                  | What it hardens                                            |
|-------------------------|------------------------------------------------------------|
| `decode_string`         | varint length + UTF-8 validation                           |
| `decode_vec_u8`         | byte-run fast path (`u8::deserialize_many`)                |
| `decode_tuple`          | mixed primitive + length-prefixed shape                    |
| `decode_collection`     | `HashMap<String, Vec<u8>>` — count cap + per-entry decode  |
| `decode_view_str`       | zero-copy `&str` decode + lifetime / UTF-8 validation      |
| `decode_struct_derive`  | derive-generated struct deserialiser                       |
| `decode_enum_derive`    | derive-generated enum deserialiser + variant-index varint  |
| `decode_versioned`      | schema-evolution body-length cap                           |

## Contract

Every target asserts the same thing on every input: the decoder must
not panic, must not read past the input slice, and must not allocate
above `Config::max_alloc`. Any failure is a bug in pack-io and should
be filed with the offending input attached.

The CI workflow runs every target for 30 seconds on every push to
`main` as a smoke check. Longer continuous fuzzing happens out-of-band
(typically on dedicated infrastructure or via [ossfuzz](https://github.com/google/oss-fuzz)
in the post-1.0 lifecycle).
