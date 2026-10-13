use pack_io::DeserializeView;

#[derive(DeserializeView)]
struct V<'a, 'b> {
    a: &'a str,
    b: &'b str,
}

fn main() {}
