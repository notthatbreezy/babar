fn main() {
    let _ = babar::query!(
        "SELECT 1::int4 AS id",
        params = (),
        row = (babar::codec::text,),
    );
}
