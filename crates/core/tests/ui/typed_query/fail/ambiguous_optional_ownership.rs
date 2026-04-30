use babar::query::Query;

fn main() {
    let _query: Query<(Option<i32>,), (i32,)> = babar::query!(
        schema = {
            table public.users {
                id: int4,
            },
        },
        SELECT users.id FROM users WHERE users.id = -$user_id?
    );
}
