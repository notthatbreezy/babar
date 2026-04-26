#[derive(babar::Codec)]
struct MixedOverride {
    id: i32,
    name: String,
    #[pg(codec = "varchar")]
    label: String,
    note: Option<String>,
}

fn main() {
    let _ = MixedOverride::CODEC;
}
