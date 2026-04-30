use babar::query::Query;

fn main() {
    let _query: Query<(i32,), (String,)> = babar::query!(
        "SELECT name FROM users WHERE id = $1",
        params = (babar::codec::int4,),
        row = (babar::codec::text,),
    );
}
