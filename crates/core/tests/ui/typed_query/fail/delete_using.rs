use babar::query::Command;

fn main() {
    let _command: Command<(i32,)> = babar::typed_query!(
        schema = {
            table public.users {
                id: int4,
            },
            table public.audit_users {
                id: int4,
            },
        },
        DELETE FROM users USING audit_users WHERE users.id = $id
    );
}
