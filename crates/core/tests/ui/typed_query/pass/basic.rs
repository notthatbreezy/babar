use babar::query::Query;

fn main() {
    let _query: Query<(i32,), (i32, String)> = babar::query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        SELECT users.id, users.name FROM users WHERE users.id = $id AND users.active = true
    );
}
