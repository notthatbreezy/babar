use babar::query::Command;

fn main() {
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
