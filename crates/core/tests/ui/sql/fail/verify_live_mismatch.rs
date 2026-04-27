use babar::sql;

fn main() {
    let _ = sql!("SELECT $id::int4", id = babar::codec::int8);
}
