use babar::query::Query;

#[derive(Clone, Debug, babar::Codec)]
struct UserRow {
    id: i32,
}

fn main() {
    let _query: Query<(), UserRow> = babar::query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        row = UserRow,
        SELECT users.id, users.name FROM users
    );
}
