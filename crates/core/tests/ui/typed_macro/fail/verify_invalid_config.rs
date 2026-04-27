fn main() {
    let _ = babar::query!(
        "SELECT $1::int4 AS id",
        params = (babar::codec::int4,),
        row = (babar::codec::int4,),
    );
}
