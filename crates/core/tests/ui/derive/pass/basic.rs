use babar::query::Query;

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserRow {
    #[pg(codec = "int4")]
    id: i32,
    #[pg(codec = "text")]
    name: String,
}

fn main() {
    let _query: Query<(), UserRow> = Query::raw("SELECT id, name FROM users", UserRow::CODEC);
}
