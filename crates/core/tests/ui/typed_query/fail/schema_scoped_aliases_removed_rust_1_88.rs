use babar::query::{Command, Query};

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
        },
    }
}

fn main() {
    let _query: Query<(), (i32,)> = app_schema::typed_query!(SELECT users.id FROM users);
    let _command: Command<(i32, String)> = app_schema::typed_command!(
        INSERT INTO users (id, name) VALUES ($id, $name)
    );
}
