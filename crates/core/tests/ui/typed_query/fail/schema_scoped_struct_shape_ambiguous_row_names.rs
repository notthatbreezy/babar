use babar::query::Query;

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        },
    }
}

#[derive(Clone, Debug, babar::Codec)]
struct UserRow {
    id: i32,
}

fn main() {
    let _query: Query<(), UserRow> = app_schema::query!(
        row = UserRow,
        SELECT users.id, users.id AS id FROM users
    );
}
