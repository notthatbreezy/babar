use babar::query::Command;

fn main() {
    let _command: Command<(String,)> = babar::typed_query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
            },
        },
        UPDATE users SET name = $name
    );
}
