use babar::query::Command;

#[derive(Clone, Debug, babar::Codec)]
struct NewUser {
    id: String,
    name: String,
    active: Option<bool>,
}

fn main() {
    let _command: Command<NewUser> = babar::command!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        params = NewUser,
        INSERT INTO users (id, name, active) VALUES ($id, $name, $active)
    );
}
