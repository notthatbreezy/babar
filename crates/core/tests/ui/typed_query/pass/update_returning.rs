use babar::query::Query;

fn main() {
    let _query: Query<(String, i32), (i32, String)> = babar::command!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        UPDATE users SET name = $name WHERE users.id = $id RETURNING users.id, users.name
    );
}
