use babar::query::Query;

fn main() {
    let _query: Query<(), (i32,)> = babar::query!(
        schema = {
            table public.users {
                id: int4,
            },
        },
        SELECT $user_id? FROM users
    );
}
