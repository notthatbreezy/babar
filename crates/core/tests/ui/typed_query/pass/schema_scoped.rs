use babar::query::Query;

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        },
        table public.posts {
            id: pk(int8),
            author_id: int4,
            title: text,
        },
    }
}

fn main() {
    let _query: Query<(i32,), (i32, String)> = app_schema::typed_query!(
        SELECT users.id, users.name FROM users WHERE users.id = $id AND users.active = true
    );
}
