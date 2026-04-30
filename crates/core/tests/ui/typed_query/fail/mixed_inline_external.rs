use babar::query::Query;

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
        },
    }
}

fn main() {
    let _query: Query<(), (i32, String)> = app_schema::query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
            },
        },
        SELECT users.id, users.name FROM users
    );
}
