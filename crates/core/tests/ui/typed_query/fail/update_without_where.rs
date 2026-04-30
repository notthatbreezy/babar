use babar::query::Command;

fn main() {
    let _command: Command<(String,)> = babar::command!(
        schema = {
            table public.users {
                id: int4,
                name: text,
            },
        },
        UPDATE users SET name = $name
    );
}
