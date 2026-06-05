//! Round-trip every collection type pack-io supports, plus a
//! demonstration of the canonical-encoding property: a `HashMap` and a
//! `BTreeMap` holding the same logical data encode to the **same** bytes.
//!
//! Run with: `cargo run --example collections_tour --release`

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use pack_io::{decode, encode};

fn show<T>(label: &str, value: T) -> Result<(), Box<dyn std::error::Error>>
where
    T: pack_io::Serialize + pack_io::Deserialize + core::fmt::Debug + PartialEq,
{
    let bytes = encode(&value)?;
    let back: T = decode(&bytes)?;
    println!("  {label:>40} → {:>4} byte(s)", bytes.len());
    assert_eq!(back, value, "round-trip mismatch for {label}");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Vec<T>");
    show("Vec<u32> (empty)", Vec::<u32>::new())?;
    show("Vec<u32> = [1, 2, 3, 4, 5]", vec![1u32, 2, 3, 4, 5])?;
    show(
        "Vec<String> = three items",
        vec![
            String::from("hello"),
            String::from("from"),
            String::from("pack-io"),
        ],
    )?;

    println!("\nBTreeMap<K, V>");
    let mut btm: BTreeMap<String, u32> = BTreeMap::new();
    let _ = btm.insert(String::from("alpha"), 1);
    let _ = btm.insert(String::from("beta"), 2);
    let _ = btm.insert(String::from("gamma"), 3);
    show("BTreeMap<String, u32>", btm.clone())?;

    println!("\nBTreeSet<T>");
    let mut bts: BTreeSet<u64> = BTreeSet::new();
    let _ = bts.insert(1);
    let _ = bts.insert(7);
    let _ = bts.insert(42);
    show("BTreeSet<u64>", bts)?;

    println!("\nHashMap<K, V>");
    let mut hm: HashMap<String, u32> = HashMap::new();
    let _ = hm.insert(String::from("alpha"), 1);
    let _ = hm.insert(String::from("beta"), 2);
    let _ = hm.insert(String::from("gamma"), 3);
    show("HashMap<String, u32>", hm.clone())?;

    println!("\nHashSet<T>");
    let mut hs: HashSet<u32> = HashSet::new();
    let _ = hs.insert(10);
    let _ = hs.insert(20);
    let _ = hs.insert(30);
    show("HashSet<u32>", hs)?;

    println!("\nCanonical encoding — HashMap and BTreeMap with the same logical");
    println!("data produce the same bytes regardless of insertion order:");

    // Insert into HashMap in one order, BTreeMap in another. Both encode the
    // same. This is the property that makes content-addressing safe.
    let mut hm2: HashMap<&str, u32> = HashMap::new();
    let _ = hm2.insert("zeta", 26);
    let _ = hm2.insert("alpha", 1);
    let _ = hm2.insert("mu", 13);

    let mut btm2: BTreeMap<&str, u32> = BTreeMap::new();
    let _ = btm2.insert("alpha", 1);
    let _ = btm2.insert("mu", 13);
    let _ = btm2.insert("zeta", 26);

    let hm2_bytes = encode(&hm2)?;
    let btm2_bytes = encode(&btm2)?;

    println!("  HashMap     ({} bytes): {:?}", hm2_bytes.len(), hm2_bytes);
    println!(
        "  BTreeMap    ({} bytes): {:?}",
        btm2_bytes.len(),
        btm2_bytes
    );
    assert_eq!(
        hm2_bytes, btm2_bytes,
        "canonical encoding must produce identical bytes"
    );
    println!("  bytes are identical ✓");

    println!("\nNested example — Vec<HashMap<String, u32>>");
    let nested = vec![hm.clone(), HashMap::new(), {
        let mut x: HashMap<String, u32> = HashMap::new();
        let _ = x.insert(String::from("single"), 999);
        x
    }];
    show("Vec<HashMap<String, u32>>", nested)?;

    println!("\ndone — every collection round-tripped");
    Ok(())
}
