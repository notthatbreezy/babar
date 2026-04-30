use babar::query::Query;

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            deleted_at: timestamptz,
        },
    }
}

fn main() {
    let _query: Query<(), (::time::OffsetDateTime,)> =
        app_schema::query!(SELECT users.deleted_at FROM users);
}
