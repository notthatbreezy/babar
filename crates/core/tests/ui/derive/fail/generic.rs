#[derive(babar::Codec)]
struct Generic<T> {
    #[pg(codec = "int4")]
    value: T,
}

fn main() {}
