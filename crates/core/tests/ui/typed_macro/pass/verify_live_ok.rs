use babar::query::Query;

fn main() {
    let _query: Query<(i32,), (i32, String)> = babar::query!(
        "SELECT $1::int4 AS id, 'ok'::text AS name",
        params = (babar::codec::int4,),
        row = (babar::codec::int4, babar::codec::text),
    );
}
