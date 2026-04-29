use babar::query::Query;

fn main() {
    let _query: Query<(), (String,)> = babar::typed_query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
            },
        },
        SELECT users.handle FROM users
    );
}
