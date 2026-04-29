use babar::query::Query;

fn main() {
    let _query: Query<(Option<i64>,), (i32,)> = babar::typed_query!(
        schema = {
            table public.users {
                id: int4,
            },
        },
        SELECT users.id FROM users LIMIT ($limit?)?
    );
}
