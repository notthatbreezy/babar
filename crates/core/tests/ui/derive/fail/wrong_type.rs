#[derive(babar::Codec)]
struct WrongType {
    #[pg(codec = "int4")]
    value: String,
}

fn main() {
    let _ = WrongType::CODEC;
}
