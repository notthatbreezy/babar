use babar::sql;

fn main() {
    let _ = sql!("SELECT $id, $name", id = babar::codec::int4);
}
