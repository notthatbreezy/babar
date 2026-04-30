use babar::query::Query;

fn main() {
    let _query: Query<(i32,), (i32,)> = babar::typed_query!(
        schema = {
            table public.users {
                id: int4,
            },
        },
        DELETE FROM users WHERE users.id = $id RETURNING *
    );
}
