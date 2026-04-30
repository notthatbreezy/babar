use babar::query::Command;

fn main() {
    let _command: Command<(bool,)> = babar::typed_query!(
        schema = {
            table public.users {
                id: int4,
                active: bool,
            },
        },
        INSERT INTO users (id) SELECT users.id FROM users WHERE users.active = $active
    );
}
