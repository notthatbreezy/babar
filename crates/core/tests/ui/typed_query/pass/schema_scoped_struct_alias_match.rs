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
struct LookupArgs {
    active: bool,
    id: i32,
}

#[derive(Clone, Debug, babar::Codec)]
struct UserRow {
    id: i32,
    display_name: String,
}

fn main() {
    let _query: Query<LookupArgs, UserRow> = app_schema::query!(
        params = LookupArgs,
        row = UserRow,
        SELECT users.name AS display_name, users.id
        FROM users
        WHERE users.id = $id AND users.active = $active
    );
}
