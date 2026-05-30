//! A tour of every primitive type pack-io supports in `v0.2.0`.
//!
//! Each section encodes a representative value, prints the encoded byte
//! count, and decodes it back. The goal is to give a reader a feel for what
//! the wire shape looks like for each kind of value (varint integers,
//! fixed-byte floats, length-prefixed strings, …).
//!
//! Run with: `cargo run --example primitive_tour --release`

use pack_io::{decode, encode};

fn show<T>(label: &str, value: T) -> Result<(), pack_io::SerialError>
where
    T: pack_io::Serialize + pack_io::Deserialize + core::fmt::Debug + PartialEq,
{
    let bytes = encode(&value)?;
    let back: T = decode(&bytes)?;
    println!("  {label:>32} → {:>3} byte(s)", bytes.len());
    assert_eq!(back, value, "round-trip mismatch for {label}");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("integers");
    show("u8 = 7", 7u8)?;
    show("u16 = 7", 7u16)?;
    show("u32 = 7", 7u32)?;
    show("u64 = 7", 7u64)?;
    show("u64 = 1_000_000", 1_000_000u64)?;
    show("u64 = u64::MAX", u64::MAX)?;
    show("i32 = -1", -1i32)?;
    show("i64 = -100_000", -100_000i64)?;
    show("u128 = u128::MAX", u128::MAX)?;

    println!("\nbool");
    show("bool = true", true)?;
    show("bool = false", false)?;

    println!("\nfloats");
    show("f32 = 1.5", 1.5f32)?;
    show("f64 = 3.141592", core::f64::consts::PI)?;

    println!("\nstrings and bytes");
    show("String = \"\"", String::new())?;
    show("String = \"pack-io\"", String::from("pack-io"))?;
    show("Vec<u8> = [0xaa; 8]", vec![0xaa_u8; 8])?;

    println!("\narrays and tuples");
    show("[u32; 4] = [1, 2, 3, 4]", [1u32, 2, 3, 4])?;
    show("tuple (u8, bool, String)", (9u8, true, String::from("hi")))?;
    show("unit = ()", ())?;

    println!("\noption and result");
    show("Option<u64> = None", None::<u64>)?;
    show("Option<u64> = Some(42)", Some(42u64))?;
    show("Result<u8, String> = Ok(1)", Ok::<u8, String>(1))?;
    show(
        "Result<u8, String> = Err(\"no\")",
        Err::<u8, String>(String::from("no")),
    )?;

    println!("\ndone — every primitive round-tripped");
    Ok(())
}
