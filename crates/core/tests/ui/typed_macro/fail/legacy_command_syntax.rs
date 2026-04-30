use babar::query::Command;

fn main() {
    let _command: Command<(i32, String)> = babar::command!(
        "INSERT INTO users (id, name) VALUES ($1, $2)",
        params = (babar::codec::int4, babar::codec::text),
    );
}
