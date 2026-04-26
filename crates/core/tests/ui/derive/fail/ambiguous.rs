#[derive(babar::Codec)]
struct Ambiguous {
    id: i32,
    payload: serde_json::Value,
}

fn main() {}
