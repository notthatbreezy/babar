//! M1 integration tests against a real Postgres container.
//!
//! These exercise the typed [`Session::execute`] / [`Session::query`]
//! surface end-to-end — encoding parameters, sending Parse/Bind/Describe/
//! Execute/Sync, decoding rows through codec values, and validating the
//! `RowDescription` column count.

mod common;

use babar::codec::{bool, bytea, float8, int4, int8, nullable, text};
use babar::query::{Command, Query};
use babar::{Error, Session};
use common::{AuthMode, PgContainer};

type Tri = (f64, i64, Vec<u8>);

fn require_docker() -> bool {
    let ok = std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
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

#[tokio::test]
async fn select_one_int_with_param() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    // SELECT $1::int4 — round-trips an integer through the parameter and
    // result paths.
    let q: Query<(i32,), (i32,)> = Query::raw("SELECT $1::int4", (int4,), (int4,));
    let rows = session.query(&q, (42_i32,)).await.expect("query");
    assert_eq!(rows, vec![(42_i32,)]);

    session.close().await.expect("close");
}

#[tokio::test]
async fn select_two_columns_decodes_tuple() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let q: Query<(i32, String), (i32, String)> =
        Query::raw("SELECT $1::int4, $2::text", (int4, text), (int4, text));
    let rows = session
        .query(&q, (7_i32, "hello".to_string()))
        .await
        .expect("query");
    assert_eq!(rows, vec![(7_i32, "hello".to_string())]);

    session.close().await.expect("close");
}

#[tokio::test]
async fn nullable_decodes_sql_null() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    // Server-emitted NULL for a nullable int column.
    let q: Query<(), (Option<i32>,)> = Query::raw("SELECT NULL::int4", (), (nullable(int4),));
    let rows = session.query(&q, ()).await.expect("query");
    assert_eq!(rows, vec![(None,)]);

    // Round-trip a Some.
    let q: Query<(Option<i32>,), (Option<i32>,)> =
        Query::raw("SELECT $1::int4", (nullable(int4),), (nullable(int4),));
    let rows = session.query(&q, (Some(99_i32),)).await.expect("query");
    assert_eq!(rows, vec![(Some(99_i32),)]);

    let rows = session.query(&q, (None::<i32>,)).await.expect("query");
    assert_eq!(rows, vec![(None,)]);

    session.close().await.expect("close");
}

#[tokio::test]
async fn create_insert_select_workflow() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE babar_users (id int4 PRIMARY KEY, name text NOT NULL, active bool)",
        (),
    );
    let affected = session.execute(&create, ()).await.expect("create");
    assert_eq!(affected, 0, "DDL reports no affected rows");

    let insert: Command<(i32, String, core::primitive::bool)> = Command::raw(
        "INSERT INTO babar_users (id, name, active) VALUES ($1, $2, $3)",
        (int4, text, bool),
    );
    let n = session
        .execute(&insert, (1_i32, "alice".to_string(), true))
        .await
        .expect("insert 1");
    assert_eq!(n, 1);
    let n = session
        .execute(&insert, (2_i32, "bob".to_string(), false))
        .await
        .expect("insert 2");
    assert_eq!(n, 1);
    let n = session
        .execute(&insert, (3_i32, "carol".to_string(), true))
        .await
        .expect("insert 3");
    assert_eq!(n, 1);

    let select: Query<(), (i32, String, core::primitive::bool)> = Query::raw(
        "SELECT id, name, active FROM babar_users ORDER BY id",
        (),
        (int4, text, bool),
    );
    let rows = session.query(&select, ()).await.expect("select");
    assert_eq!(
        rows,
        vec![
            (1, "alice".to_string(), true),
            (2, "bob".to_string(), false),
            (3, "carol".to_string(), true),
        ]
    );

    let update: Command<(core::primitive::bool, i32)> = Command::raw(
        "UPDATE babar_users SET active = $1 WHERE id = $2",
        (bool, int4),
    );
    let n = session
        .execute(&update, (false, 1_i32))
        .await
        .expect("update");
    assert_eq!(n, 1);

    let delete: Command<(i32,)> = Command::raw("DELETE FROM babar_users WHERE id = $1", (int4,));
    let n = session.execute(&delete, (3_i32,)).await.expect("delete");
    assert_eq!(n, 1);

    session.close().await.expect("close");
}

#[tokio::test]
async fn column_alignment_mismatch_surfaces_as_error() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    // Server returns 2 columns but the decoder only consumes 1 — surface
    // the mismatch before any decode runs.
    // With binary format codes the server itself may reject the Bind message
    // ("bind message has N result formats but query has M columns"), which is
    // equally valid.
    let q: Query<(), (i32,)> = Query::raw("SELECT 1::int4, 2::int4", (), (int4,));
    let err = session.query(&q, ()).await.expect_err("must mismatch");
    match &err {
        Error::ColumnAlignment {
            expected, actual, ..
        } => {
            assert_eq!(*expected, 1);
            assert_eq!(*actual, 2);
        }
        Error::Server { code, .. } => {
            // 08P01 = protocol_violation — server caught format count mismatch
            assert_eq!(code, "08P01");
        }
        other => panic!("expected ColumnAlignment or Server protocol error, got {other:?}"),
    }

    // Same in the other direction — need a fresh session since server error
    // may have poisoned the connection state.
    let Some((_pg2, session2)) = fresh_session().await else {
        return;
    };
    let q: Query<(), (i32, i32)> = Query::raw("SELECT 1::int4", (), (int4, int4));
    let err = session2.query(&q, ()).await.expect_err("must mismatch");
    match &err {
        Error::ColumnAlignment {
            expected, actual, ..
        } => {
            assert_eq!(*expected, 2);
            assert_eq!(*actual, 1);
        }
        Error::Server { code, .. } => {
            assert_eq!(code, "08P01");
        }
        other => panic!("expected ColumnAlignment or Server protocol error, got {other:?}"),
    }

    session2.close().await.expect("close");
}

#[tokio::test]
async fn float_and_bigint_and_bytea_roundtrip() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let q: Query<Tri, Tri> = Query::raw(
        "SELECT $1::float8, $2::int8, $3::bytea",
        (float8, int8, bytea),
        (float8, int8, bytea),
    );
    let v = (
        std::f64::consts::PI,
        9_999_999_999_i64,
        vec![0xDE_u8, 0xAD, 0xBE, 0xEF],
    );
    let rows = session.query(&q, v.clone()).await.expect("query");
    assert_eq!(rows.len(), 1);
    let (got_f, got_i, got_b) = &rows[0];
    assert_eq!(got_f.to_bits(), v.0.to_bits());
    assert_eq!(*got_i, v.1);
    assert_eq!(*got_b, v.2);

    session.close().await.expect("close");
}

#[tokio::test]
async fn server_error_during_extended_propagates() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    // Bad cast — server returns ErrorResponse, ReadyForQuery 'I'. The
    // driver must surface the server error and stay usable for the next
    // command.
    let q: Query<(), (i32,)> = Query::raw("SELECT 'not an int'::int4", (), (int4,));
    match session.query(&q, ()).await {
        Err(Error::Server { code, .. }) => {
            // 22P02 = invalid_text_representation
            assert_eq!(code, "22P02", "expected invalid_text_representation");
        }
        other => panic!("expected Error::Server, got {other:?}"),
    }

    // Session should still work afterwards.
    let q: Query<(), (i32,)> = Query::raw("SELECT 1::int4", (), (int4,));
    assert_eq!(session.query(&q, ()).await.unwrap(), vec![(1,)]);

    session.close().await.expect("close");
}
