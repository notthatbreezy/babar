use babar::query::Command;

fn main() {
    let _command: Command<(i32,)> = babar::command!(
        schema = {
            table public.users {
                id: int4,
            },
        },
        INSERT INTO users (id) VALUES ($id) ON CONFLICT (id) DO NOTHING
    );
}
