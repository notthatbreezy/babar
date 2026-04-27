use babar::query::{Command, Query};

fn main() {
    let _query: Query<(i32,), (String,)> = babar::query!(
        "SELECT name FROM users WHERE id = $1",
        params = (int4,),
        row = (text,),
    );

    let _command: Command<(i32, String)> = babar::command!(
        "INSERT INTO users (id, name) VALUES ($1, $2)",
        params = (int4, text),
    );
}
