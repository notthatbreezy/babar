use babar::codec::int4;
use babar::sql;

fn main() {
    let _ = sql!("SELECT $id::int4", id = int4);
}
