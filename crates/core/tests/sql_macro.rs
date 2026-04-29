//! `sql!` integration tests.

mod common;

use babar::codec::{bool, int4, int8, text};
use babar::query::{Command, Fragment, Query};
use babar::sql;
use babar::types;
use babar::Session;
use common::{AuthMode, PgContainer};

fn require_docker() -> bool {
    let ok = std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !ok {
        eprintln!("skipping: docker unavailable");
    }
    ok
}

async fn fresh_session() -> Option<(PgContainer, Session)> {
    if !require_docker() {
        return None;
    }
    let pg = PgContainer::start(AuthMode::Scram).await;
    let session = Session::connect(pg.config(pg.user(), pg.password()))
        .await
        .expect("connect");
    Some((pg, session))
}

#[test]
fn named_placeholders_build_flat_fragment() {
    let fragment: Fragment<(i32, String)> = sql!(
        "SELECT * FROM users WHERE id = $id OR name = $name OR owner_id = $id",
        id = int4,
        name = text,
    );

    assert_eq!(
        fragment.sql(),
        "SELECT * FROM users WHERE id = $1 OR name = $2 OR owner_id = $1"
    );
    assert_eq!(fragment.n_params(), 2);
    assert_eq!(fragment.param_oids(), &[types::INT4, types::TEXT],);

    let origin = fragment.origin().expect("macro captures origin");
    assert!(origin.file().ends_with("crates/core/tests/sql_macro.rs"));

    let builder = Fragment::lit("SELECT * FROM users WHERE id = ")
        .bind(int4)
        .append_lit(" OR name = ")
        .bind(text)
        .append_lit(" OR owner_id = $1");
    assert_eq!(fragment.sql(), builder.sql());
    assert_eq!(fragment.n_params(), builder.n_params());
    assert_eq!(fragment.param_oids(), builder.param_oids());
}

#[test]
fn nested_sql_fragments_flatten_parameter_order() {
    let fragment: Fragment<(i32, String, bool)> = sql!(
        "SELECT * FROM users WHERE $predicate AND active = $active",
        predicate = sql!("id = $id AND name = $name", id = int4, name = text),
        active = bool,
    );

    assert_eq!(
        fragment.sql(),
        "SELECT * FROM users WHERE id = $1 AND name = $2 AND active = $3"
    );
    assert_eq!(fragment.n_params(), 3);
    assert_eq!(
        fragment.param_oids(),
        &[types::INT4, types::TEXT, types::BOOL],
    );

    let composed = Fragment::lit("SELECT * FROM users WHERE ")
        .plus(
            Fragment::lit("id = ")
                .bind(int4)
                .append_lit(" AND name = ")
                .bind(text),
        )
        .append_lit(" AND active = ")
        .bind(bool);
    assert_eq!(fragment.sql(), composed.sql());
    assert_eq!(fragment.n_params(), composed.n_params());
    assert_eq!(fragment.param_oids(), composed.param_oids());
}

#[test]
fn sql_macro_query_and_command_match_raw_builders() {
    let macro_query: Query<(i32, bool), (i32, String)> = sql!(
        "SELECT id, name FROM users WHERE id = $id AND active = $active",
        id = int4,
        active = bool,
    )
    .query((int4, text));
    let raw_query: Query<(i32, bool), (i32, String)> = Query::raw(
        "SELECT id, name FROM users WHERE id = $1 AND active = $2",
        (int4, bool),
        (int4, text),
    );
    assert_eq!(macro_query.sql(), raw_query.sql());
    assert_eq!(macro_query.param_oids(), raw_query.param_oids());
    assert_eq!(macro_query.output_oids(), raw_query.output_oids());

    let macro_command: Command<(i32, String)> = sql!(
        "INSERT INTO users (id, name) VALUES ($id, $name)",
        id = int4,
        name = text,
    )
    .command();
    let raw_command: Command<(i32, String)> =
        Command::raw("INSERT INTO users (id, name) VALUES ($1, $2)", (int4, text));
    assert_eq!(macro_command.sql(), raw_command.sql());
    assert_eq!(macro_command.param_oids(), raw_command.param_oids());
}

#[test]
fn typed_query_and_command_macros_match_raw_builders() {
    let macro_query: Query<(i32, bool), (i32, String)> = babar::query!(
        "SELECT id, name FROM users WHERE id = $1 AND active = $2",
        params = (int4, bool),
        row = (int4, text),
    );
    let raw_query: Query<(i32, bool), (i32, String)> = Query::raw(
        "SELECT id, name FROM users WHERE id = $1 AND active = $2",
        (int4, bool),
        (int4, text),
    );
    assert_eq!(macro_query.sql(), raw_query.sql());
    assert_eq!(macro_query.param_oids(), raw_query.param_oids());
    assert_eq!(macro_query.output_oids(), raw_query.output_oids());
    let origin = macro_query.origin().expect("macro captures origin");
    assert!(origin.file().ends_with("crates/core/tests/sql_macro.rs"));

    let macro_command: Command<(i32, String)> = babar::command!(
        "INSERT INTO users (id, name) VALUES ($1, $2)",
        params = (int4, text),
    );
    let raw_command: Command<(i32, String)> =
        Command::raw("INSERT INTO users (id, name) VALUES ($1, $2)", (int4, text));
    assert_eq!(macro_command.sql(), raw_command.sql());
    assert_eq!(macro_command.param_oids(), raw_command.param_oids());
    let origin = macro_command.origin().expect("macro captures origin");
    assert!(origin.file().ends_with("crates/core/tests/sql_macro.rs"));
}

#[test]
fn typed_query_macro_matches_raw_builder() {
    let macro_query: Query<(i32,), (i32, String)> = babar::typed_query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        SELECT users.id, users.name FROM users WHERE users.id = $id AND users.active = true
    );
    let raw_query: Query<(i32,), (i32, String)> = Query::raw(
        "SELECT users.id, users.name FROM users AS users WHERE ((users.id = $1) AND (users.active = TRUE))",
        (int4,),
        (int4, text),
    );
    assert_eq!(macro_query.sql(), raw_query.sql());
    assert_eq!(macro_query.param_oids(), raw_query.param_oids());
    assert_eq!(macro_query.output_oids(), raw_query.output_oids());
    let origin = macro_query.origin().expect("macro captures origin");
    assert!(origin.file().ends_with("crates/core/tests/sql_macro.rs"));
}

#[test]
fn typed_query_optional_suffixes_match_all_active_raw_builder() {
    let macro_query: Query<(i32, i32, bool, i64, i64), (String,)> = babar::typed_query!(
        schema = {
            table public.users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        SELECT users.name
        FROM users
        WHERE (users.id >= $min_id? AND users.id <= $max_id?)?
          AND (users.active = $active?)?
        ORDER BY users.id
        LIMIT $limit?
        OFFSET $offset?
    );
    let raw_query: Query<(i32, i32, bool, i64, i64), (String,)> = Query::raw(
        "SELECT users.name FROM users AS users WHERE ((((users.id >= $1) AND (users.id <= $2))) AND ((users.active = $3))) ORDER BY users.id LIMIT $4 OFFSET $5",
        (int4, int4, bool, int8, int8),
        (text,),
    );

    assert_eq!(macro_query.sql(), raw_query.sql());
    assert_eq!(macro_query.param_oids(), raw_query.param_oids());
    assert_eq!(macro_query.output_oids(), raw_query.output_oids());
}

#[tokio::test]
async fn sql_macro_fragments_execute_against_postgres() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    session
        .simple_query_raw(
            "CREATE TEMP TABLE sql_macro_users (\
                id int4 PRIMARY KEY, \
                owner_id int4 NOT NULL, \
                name text NOT NULL, \
                active bool NOT NULL\
            )",
        )
        .await
        .expect("create table");

    let insert: Command<(i32, i32, String, bool)> = sql!(
        "INSERT INTO sql_macro_users (id, owner_id, name, active) \
         VALUES ($id, $owner_id, $name, $active)",
        id = int4,
        owner_id = int4,
        name = text,
        active = bool,
    )
    .command();
    for row in [
        (1, 9, "alice".to_string(), true),
        (2, 1, "bob".to_string(), false),
        (3, 1, "carol".to_string(), true),
    ] {
        let affected = session.execute(&insert, row).await.expect("insert row");
        assert_eq!(affected, 1);
    }

    let select: Query<(i32, bool), (String,)> = sql!(
        "SELECT name FROM sql_macro_users \
             WHERE ($predicate) AND active = $active \
             ORDER BY id",
        predicate = sql!("id = $id OR owner_id = $id", id = int4),
        active = bool,
    )
    .query((text,));
    let rows = session
        .query(&select, (1_i32, true))
        .await
        .expect("select rows");
    assert_eq!(rows, vec![("alice".to_string(),), ("carol".to_string(),)]);

    session.close().await.expect("close");
}

#[tokio::test]
async fn typed_query_and_command_macros_execute_against_postgres() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    session
        .simple_query_raw(
            "CREATE TEMP TABLE typed_macro_users (\
                id int4 PRIMARY KEY, \
                name text NOT NULL, \
                note text NULL\
            )",
        )
        .await
        .expect("create table");

    let insert: Command<(i32, String, Option<String>)> = babar::command!(
        "INSERT INTO typed_macro_users (id, name, note) VALUES ($1, $2, $3)",
        params = (int4, text, nullable(text)),
    );
    for row in [
        (1, "alice".to_string(), Some("first".to_string())),
        (2, "bob".to_string(), None),
    ] {
        let affected = session.execute(&insert, row).await.expect("insert row");
        assert_eq!(affected, 1);
    }

    let select: Query<(i32,), (String, Option<String>)> = babar::query!(
        "SELECT name, note FROM typed_macro_users WHERE id >= $1 ORDER BY id",
        params = (int4,),
        row = (text, nullable(text)),
    );
    let rows = session.query(&select, (1_i32,)).await.expect("select rows");
    assert_eq!(
        rows,
        vec![
            ("alice".to_string(), Some("first".to_string())),
            ("bob".to_string(), None),
        ]
    );

    session.close().await.expect("close");
}

#[tokio::test]
async fn typed_query_macro_executes_against_postgres() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    session
        .simple_query_raw(
            "CREATE TEMP TABLE typed_query_users (\
                id int4 PRIMARY KEY, \
                name text NOT NULL, \
                active bool NOT NULL\
            )",
        )
        .await
        .expect("create table");

    let insert: Command<(i32, String, bool)> = Command::raw(
        "INSERT INTO typed_query_users (id, name, active) VALUES ($1, $2, $3)",
        (int4, text, bool),
    );
    for row in [
        (1, "alice".to_string(), true),
        (2, "bob".to_string(), false),
        (3, "carol".to_string(), true),
    ] {
        let affected = session.execute(&insert, row).await.expect("insert row");
        assert_eq!(affected, 1);
    }

    let select: Query<(i32,), (String,)> = babar::typed_query!(
        schema = {
            table typed_query_users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        SELECT typed_query_users.name
        FROM typed_query_users
        WHERE typed_query_users.id >= $min_id AND typed_query_users.active = true
        ORDER BY typed_query_users.id
    );
    let rows = session.query(&select, (1_i32,)).await.expect("select rows");
    assert_eq!(rows, vec![("alice".to_string(),), ("carol".to_string(),)]);

    session.close().await.expect("close");
}

#[tokio::test]
async fn typed_query_optional_suffixes_execute_against_postgres() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    session
        .simple_query_raw(
            "CREATE TEMP TABLE typed_query_optional_users (\
                id int4 PRIMARY KEY, \
                name text NOT NULL, \
                active bool NOT NULL\
            )",
        )
        .await
        .expect("create table");

    let insert: Command<(i32, String, bool)> = Command::raw(
        "INSERT INTO typed_query_optional_users (id, name, active) VALUES ($1, $2, $3)",
        (int4, text, bool),
    );
    for row in [
        (1, "alice".to_string(), true),
        (2, "bob".to_string(), false),
        (3, "carol".to_string(), true),
        (4, "dave".to_string(), true),
    ] {
        let affected = session.execute(&insert, row).await.expect("insert row");
        assert_eq!(affected, 1);
    }

    let select: Query<(i32, i32, bool, i64, i64), (String,)> = babar::typed_query!(
        schema = {
            table typed_query_optional_users {
                id: int4,
                name: text,
                active: bool,
            },
        },
        SELECT typed_query_optional_users.name
        FROM typed_query_optional_users
        WHERE (typed_query_optional_users.id >= $min_id? AND typed_query_optional_users.id <= $max_id?)?
          AND (typed_query_optional_users.active = $active?)?
        ORDER BY typed_query_optional_users.id
        LIMIT $limit?
        OFFSET $offset?
    );
    let raw_select: Query<(i32, i32, bool, i64, i64), (String,)> = Query::raw(
        "SELECT typed_query_optional_users.name FROM typed_query_optional_users AS typed_query_optional_users WHERE ((((typed_query_optional_users.id >= $1) AND (typed_query_optional_users.id <= $2))) AND ((typed_query_optional_users.active = $3))) ORDER BY typed_query_optional_users.id LIMIT $4 OFFSET $5",
        (int4, int4, bool, int8, int8),
        (text,),
    );

    assert_eq!(select.sql(), raw_select.sql());
    assert_eq!(select.param_oids(), raw_select.param_oids());

    let rows = session
        .query(&select, (2_i32, 4_i32, true, 10_i64, 0_i64))
        .await
        .expect("select rows");
    assert_eq!(rows, vec![("carol".to_string(),), ("dave".to_string(),)]);

    session.close().await.expect("close");
}
