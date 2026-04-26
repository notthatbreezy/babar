#[derive(babar::Codec)]
struct Broken {
    #[pg(codec = "int4")]
    id: i32,
    name: String,
}

fn main() {}
