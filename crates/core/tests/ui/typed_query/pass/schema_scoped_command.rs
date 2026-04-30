use babar::query::Command;

babar::schema! {
    mod app_schema {
        table public.users {
            id: primary_key(int4),
            name: text,
            active: bool,
        },
    }
}

fn main() {
    let _command: Command<(i32, String, bool)> = app_schema::typed_command!(
        INSERT INTO users (id, name, active) VALUES ($id, $name, $active)
    );
}
