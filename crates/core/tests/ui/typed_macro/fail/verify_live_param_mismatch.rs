fn main() {
    let _ = babar::command!("SELECT $1::int4", params = (babar::codec::int8,));
}
