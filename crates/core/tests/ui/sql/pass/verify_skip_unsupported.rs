use babar::codec::bpchar;
use babar::sql;

fn main() {
    let _ = sql!("SELECT $name::bpchar", name = bpchar);
}
