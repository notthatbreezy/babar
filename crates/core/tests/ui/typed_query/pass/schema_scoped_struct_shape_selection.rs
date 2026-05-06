use babar::query::{Command, Query};

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
struct UserRow {
    id: i32,
    name: String,
}

#[derive(Clone, Debug, babar::Codec)]
struct NewUser {
    id: i32,
    name: String,
    active: bool,
}

fn main() {
    let _query_explicit: Query<LookupArgs, UserRow> = app_schema::query!(
        params = LookupArgs,
        row = UserRow,
        SELECT users.id, users.name FROM users WHERE users.id = $id AND users.active = true
    );

    let _query_inferred: Query<LookupArgs, UserRow> = app_schema::query!(
        params = _,
        row = _,
        SELECT users.id, users.name FROM users WHERE users.id = $id AND users.active = true
    );

    let _command_explicit: Command<NewUser> = app_schema::command!(
        params = NewUser,
        INSERT INTO users (id, name, active) VALUES ($id, $name, $active)
    );

    let _command_inferred: Command<NewUser> = app_schema::command!(
        params = _,
        INSERT INTO users (id, name, active) VALUES ($id, $name, $active)
    );
}
