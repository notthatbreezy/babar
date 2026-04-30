use babar::query::{Command, Query};

fn main() {
    let _query: Query<(i32,), (i32, String)> = babar::query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        SELECT users.id, users.name FROM users WHERE users.id = $id AND users.active = true
    );

    let _command: Command<(i32, String, bool)> = babar::command!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        INSERT INTO users (id, name, active) VALUES ($id, $name, $active)
    );
}
