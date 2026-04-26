use babar::sql;

fn main() {
    let _ = sql!(
        "SELECT $id",
        id = babar::codec::int4,
        name = babar::codec::text,
    );
}
