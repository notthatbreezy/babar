use babar::query::Command;

fn main() {
    let _command: Command<(String, i32)> = babar::command!(
        schema = {
            table public.users {
                id: int4,
                name: text,
            },
            table public.audit_users {
                id: int4,
                name: text,
            },
        },
        UPDATE users SET name = $name FROM audit_users WHERE users.id = $id
    );
}
