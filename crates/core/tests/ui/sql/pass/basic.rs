use babar::codec::{bool, int4, text};
use babar::query::{Command, Fragment, Query};
use babar::sql;

fn main() {
    let fragment: Fragment<(i32, bool)> = sql!(
        "SELECT name FROM users WHERE ($filter) AND active = $active",
        filter = sql!("id = $id OR owner_id = $id", id = int4),
        active = bool,
    );
    assert_eq!(
        fragment.sql(),
        "SELECT name FROM users WHERE (id = $1 OR owner_id = $1) AND active = $2"
    );

    let _query: Query<(i32, bool), (String,)> = Query::from_fragment(fragment, (text,));
    let _command: Command<(i32, String)> = Command::from_fragment(sql!(
        "INSERT INTO users (id, name) VALUES ($id, $name)",
        id = int4,
        name = text,
    ));
}
