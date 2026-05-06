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
    id: i32,
}

#[derive(Clone, Debug, babar::Codec)]
struct WrongLookupArgs {
    name: String,
}

#[derive(Clone, Debug, babar::Codec)]
struct UserRow {
    id: i32,
    name: String,
}

#[derive(Clone, Debug, babar::Codec)]
struct WrongUserRow {
    id: i32,
}

fn main() {
    let _: Query<WrongLookupArgs, WrongUserRow> = app_schema::query!(
        params = LookupArgs,
        row = UserRow,
        SELECT users.id, users.name FROM users WHERE users.id = $id AND users.active = true
    );
}
