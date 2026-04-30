use babar::query::Command;

fn main() {
    let _command: Command<(i32,)> = babar::command!(
        schema = {
            table public.users {
                id: int4,
            },
        },
        DELETE FROM users
    );
}
