use pack_io::Serialize;

// pack-io never encodes Rust discriminants: `A` is tag 0, not 5. 1.x and
// the 2.0.0 pre-release ignored the `= 5` silently.
#[derive(Serialize)]
enum E {
    A = 5,
    B = 1,
}

fn main() {}
