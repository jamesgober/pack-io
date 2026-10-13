//! Compile-time UI tests for the derive macros.
//!
//! `tests/ui/fail/*.rs` must fail to compile with exactly the diagnostics in
//! the matching `.stderr` file: every misuse that pack-io 1.0 silently
//! accepted (unknown attributes on enum fields and variants, `since` newer
//! than the struct version, …) is now a spanned compile error.
//! `tests/ui/pass/*.rs` must compile and run: they pin derive bugs fixed in
//! 2.0 (raw identifiers, generated-name hygiene, lifetime-less views,
//! `no_std`-clean generated paths).
//!
//! The `.stderr` snapshots are rustc-version sensitive, so CI runs this file
//! on the stable toolchain only. Regenerate snapshots with
//! `TRYBUILD=overwrite cargo test --test derive_ui --features derive` and
//! review the diff.

#[test]
fn derive_ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/fail/*.rs");
    t.pass("tests/ui/pass/*.rs");
}
