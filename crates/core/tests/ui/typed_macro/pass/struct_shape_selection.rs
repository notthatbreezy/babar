use babar::query::{Command, Query};

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
    let _query_explicit: Query<LookupArgs, UserRow> = babar::query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        params = LookupArgs,
        row = UserRow,
        SELECT users.id, users.name FROM users WHERE users.id = $id AND users.active = true
    );

    let _query_inferred: Query<LookupArgs, UserRow> = babar::query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        params = _,
        row = _,
        SELECT users.id, users.name FROM users WHERE users.id = $id AND users.active = true
    );

    let _command_explicit: Command<NewUser> = babar::command!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        params = NewUser,
        INSERT INTO users (id, name, active) VALUES ($id, $name, $active)
    );

    let _command_inferred: Command<NewUser> = babar::command!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        params = _,
        INSERT INTO users (id, name, active) VALUES ($id, $name, $active)
    );
}
