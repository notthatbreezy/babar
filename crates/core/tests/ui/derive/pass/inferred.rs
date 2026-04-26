#[derive(babar::Codec)]
struct Inferred {
    id: i32,
    name: String,
    active: bool,
    payload: Vec<u8>,
    note: Option<String>,
    visits: i64,
}

fn main() {
    let _ = Inferred::CODEC;
}
