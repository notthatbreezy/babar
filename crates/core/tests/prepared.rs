//! M2 integration tests for prepared statements.
//!
//! These exercise `Session::prepare_query`, `Session::prepare_command`,
//! `PreparedQuery::query`, `PreparedCommand::execute`, caching behavior,
//! and `Drop`-triggered DEALLOCATE.

mod common;

use babar::codec::{int4, int8, text};
use babar::query::{Command, Query};
use babar::{Error, Session};
use common::{AuthMode, PgContainer};

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
async fn prepared_query_basic_roundtrip() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let q: Query<(i32,), (i32,)> = Query::raw("SELECT $1::int4", (int4,), (int4,));
    let pq = session.prepare_query(&q).await.expect("prepare");

    // Execute multiple times — reuses the same server-side statement.
    let rows = pq.query((42_i32,)).await.expect("query 1");
    assert_eq!(rows, vec![(42,)]);

    let rows = pq.query((99_i32,)).await.expect("query 2");
    assert_eq!(rows, vec![(99,)]);

    pq.close().await.expect("close prepared");
    session.close().await.expect("close session");
}

#[tokio::test]
async fn prepared_command_insert_and_count() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    session
        .simple_query_raw("CREATE TABLE prep_test (id int4, name text)")
        .await
        .expect("create table");

    let cmd: Command<(i32, String)> = Command::raw(
        "INSERT INTO prep_test (id, name) VALUES ($1, $2)",
        (int4, text),
    );
    let pc = session.prepare_command(&cmd).await.expect("prepare cmd");

    let affected = pc
        .execute((1_i32, "alice".to_string()))
        .await
        .expect("exec 1");
    assert_eq!(affected, 1);

    let affected = pc
        .execute((2_i32, "bob".to_string()))
        .await
        .expect("exec 2");
    assert_eq!(affected, 1);

    // Verify data.
    let q: Query<(), (i32, String)> = Query::raw(
        "SELECT id, name FROM prep_test ORDER BY id",
        (),
        (int4, text),
    );
    let rows = session.query(&q, ()).await.expect("select");
    assert_eq!(rows, vec![(1, "alice".to_string()), (2, "bob".to_string())]);

    pc.close().await.expect("close prepared cmd");
    session.close().await.expect("close session");
}

#[tokio::test]
async fn prepared_query_cache_reuses_statement() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let q: Query<(i32,), (i32,)> = Query::raw("SELECT $1::int4 + 1", (int4,), (int4,));

    // Prepare twice — should get the same server-side statement name.
    let pq1 = session.prepare_query(&q).await.expect("prepare 1");
    let pq2 = session.prepare_query(&q).await.expect("prepare 2");
    assert_eq!(pq1.name(), pq2.name());

    let rows = pq1.query((5_i32,)).await.expect("query via pq1");
    assert_eq!(rows, vec![(6,)]);

    let rows = pq2.query((10_i32,)).await.expect("query via pq2");
    assert_eq!(rows, vec![(11,)]);

    session.close().await.expect("close");
}

#[tokio::test]
async fn prepared_query_cache_keeps_statement_open_until_last_handle_closes() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let q: Query<(i32,), (i32,)> = Query::raw("SELECT $1::int4 + 1", (int4,), (int4,));
    let pq1 = session.prepare_query(&q).await.expect("prepare 1");
    let pq2 = session.prepare_query(&q).await.expect("prepare 2");
    let name = pq1.name().to_string();

    pq1.close().await.expect("close first handle");

    let rows = pq2
        .query((10_i32,))
        .await
        .expect("query via surviving handle");
    assert_eq!(rows, vec![(11,)]);

    let check_q: Query<(String,), (String,)> = Query::raw(
        "SELECT name FROM pg_prepared_statements WHERE name = $1",
        (text,),
        (text,),
    );
    let exists = session
        .query(&check_q, (name.clone(),))
        .await
        .expect("statement should still exist");
    assert_eq!(exists, vec![(name.clone(),)]);

    pq2.close().await.expect("close last handle");

    let exists = session
        .query(&check_q, (name.clone(),))
        .await
        .expect("statement should be gone");
    assert!(exists.is_empty());

    session.close().await.expect("close");
}

#[tokio::test]
async fn prepared_query_drop_deallocates_on_server() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let q: Query<(i32,), (i32,)> = Query::raw("SELECT $1::int4", (int4,), (int4,));
    let pq = session.prepare_query(&q).await.expect("prepare");
    let name = pq.name().to_string();

    // Verify the statement exists in pg_prepared_statements.
    let check_q2: Query<(String,), (String,)> = Query::raw(
        "SELECT name FROM pg_prepared_statements WHERE name = $1",
        (text,),
        (text,),
    );
    let exists = session
        .query(&check_q2, (name.clone(),))
        .await
        .expect("check exists");
    assert_eq!(exists.len(), 1, "statement should exist before drop");

    // Drop the prepared query — this should trigger DEALLOCATE.
    drop(pq);

    // Give the background task a moment to send the CloseStatement.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let exists = session
        .query(&check_q2, (name.clone(),))
        .await
        .expect("check after drop");
    assert_eq!(
        exists.len(),
        0,
        "statement should be deallocated after drop"
    );

    session.close().await.expect("close");
}

#[tokio::test]
async fn schema_mismatch_detected_at_prepare_time() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    // Server returns int4 but we declare decoder expects int8.
    let q: Query<(), (i64,)> = Query::raw("SELECT 42::int4", (), (int8,));
    let err = session
        .prepare_query(&q)
        .await
        .expect_err("should mismatch");
    match err {
        Error::SchemaMismatch {
            position,
            expected_oid,
            actual_oid,
            ..
        } => {
            assert_eq!(position, 0);
            // int8 OID = 20, int4 OID = 23
            assert_eq!(expected_oid, 20);
            assert_eq!(actual_oid, 23);
        }
        other => panic!("expected SchemaMismatch, got {other:?}"),
    }

    session.close().await.expect("close");
}

#[tokio::test]
async fn schema_mismatch_column_count() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    // Server returns 2 columns but decoder expects 1.
    let q: Query<(), (i32,)> = Query::raw("SELECT 1::int4, 2::int4", (), (int4,));
    let err = session
        .prepare_query(&q)
        .await
        .expect_err("should mismatch");
    match err {
        Error::ColumnAlignment {
            expected, actual, ..
        } => {
            assert_eq!(expected, 1);
            assert_eq!(actual, 2);
        }
        other => panic!("expected ColumnAlignment, got {other:?}"),
    }

    session.close().await.expect("close");
}
