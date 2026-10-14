use pack_io::Deserialize;

// A reader may announce `removed = version + 1` (reader-first rollout);
// further ahead is almost certainly a typo.
#[derive(Deserialize)]
#[pack_io(version = 2)]
struct S {
    a: u8,
    #[pack_io(removed = 40)]
    c: u32,
}

fn main() {}
