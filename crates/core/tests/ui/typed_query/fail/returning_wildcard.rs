use babar::query::Query;

fn main() {
    let _query: Query<(i32,), (i32,)> = babar::command!(
        schema = {
            table public.users {
                id: int4,
            },
        },
        DELETE FROM users WHERE users.id = $id RETURNING *
    );
}
