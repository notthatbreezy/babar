//! M4 integration tests for scoped transactions and nested savepoints.

mod common;

use std::time::Duration;

use babar::codec::{int4, int8, nullable, text};
use babar::query::{Command, Query};
use babar::{Error, Savepoint, Session, Transaction};
use common::{AuthMode, PgContainer};
use futures_util::{future, FutureExt};

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
    let session = Session::connect(
        pg.config(pg.user(), pg.password())
            .application_name("babar-m4-transactions"),
    )
    .await
    .expect("connect");
    Some((pg, session))
}

#[tokio::test]
async fn transaction_commits_on_ok_and_rolls_back_on_err() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE tx_demo (id int4 PRIMARY KEY, note text NOT NULL)",
        (),
    );
    session.execute(&create, ()).await.expect("create table");

    let select: Query<(), (i32, String)> =
        Query::raw("SELECT id, note FROM tx_demo ORDER BY id", (), (int4, text));

    session
        .transaction(commit_row)
        .await
        .expect("commit transaction");

    let err = session
        .transaction(rollback_row)
        .await
        .expect_err("transaction should roll back");
    assert!(matches!(err, Error::Config(_)));

    let rows = session.query(&select, ()).await.expect("select rows");
    assert_eq!(rows, vec![(1, "committed".to_string())]);

    session.close().await.expect("close");
}

#[tokio::test]
async fn dropped_transaction_future_rolls_back() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let create: Command<()> = Command::raw("CREATE TEMP TABLE dropped_tx_demo (id int4)", ());
    let count: Query<(), (i64,)> =
        Query::raw("SELECT COUNT(*)::int8 FROM dropped_tx_demo", (), (int8,));
    session.execute(&create, ()).await.expect("create table");

    {
        let fut = session.transaction(dropped_future_body);
        tokio::pin!(fut);
        let _ = &fut;

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    let rows = session.query(&count, ()).await.expect("count rows");
    assert_eq!(rows, vec![(0,)]);

    session.close().await.expect("close");
}

#[tokio::test]
async fn panic_rolls_back_and_leaves_connection_clean() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let create: Command<()> = Command::raw("CREATE TEMP TABLE panic_tx_demo (id int4)", ());
    let count: Query<(), (i64,)> =
        Query::raw("SELECT COUNT(*)::int8 FROM panic_tx_demo", (), (int8,));
    let txid: Query<(), (Option<i64>,)> = Query::raw(
        "SELECT txid_current_if_assigned()::int8",
        (),
        (nullable(int8),),
    );
    session.execute(&create, ()).await.expect("create table");

    let panic_result = std::panic::AssertUnwindSafe(session.transaction(panic_body))
        .catch_unwind()
        .await;
    assert!(panic_result.is_err(), "transaction should propagate panic");

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(
        session.query(&count, ()).await.expect("count rows"),
        vec![(0,)]
    );
    assert_eq!(session.query(&txid, ()).await.expect("txid"), vec![(None,)]);

    session.close().await.expect("close");
}

#[tokio::test]
async fn nested_savepoints_roll_back_middle_scope_and_commit_outer_scope() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let create: Command<()> =
        Command::raw("CREATE TEMP TABLE savepoint_demo (id int4, note text)", ());
    let select: Query<(), (i32, String)> = Query::raw(
        "SELECT id, note FROM savepoint_demo ORDER BY id",
        (),
        (int4, text),
    );
    session.execute(&create, ()).await.expect("create table");

    session
        .transaction(nested_savepoint_body)
        .await
        .expect("outer transaction commits");

    let rows = session.query(&select, ()).await.expect("select rows");
    assert_eq!(
        rows,
        vec![
            (1, "outer-before".to_string()),
            (4, "outer-after".to_string()),
        ]
    );

    session.close().await.expect("close");
}

async fn commit_row(tx: Transaction<'_>) -> babar::Result<()> {
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO tx_demo (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    tx.execute(&insert, (1, "committed".to_string())).await?;
    Ok(())
}

async fn rollback_row(tx: Transaction<'_>) -> babar::Result<()> {
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO tx_demo (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    tx.execute(&insert, (2, "rolled-back".to_string())).await?;
    Err(Error::Config("force rollback".into()))
}

async fn dropped_future_body(tx: Transaction<'_>) -> babar::Result<()> {
    let insert: Command<(i32,)> =
        Command::raw("INSERT INTO dropped_tx_demo (id) VALUES ($1)", (int4,));
    tx.execute(&insert, (1,)).await?;
    future::pending::<()>().await;
    #[allow(unreachable_code)]
    Ok(())
}

async fn panic_body(tx: Transaction<'_>) -> babar::Result<()> {
    let insert: Command<(i32,)> =
        Command::raw("INSERT INTO panic_tx_demo (id) VALUES ($1)", (int4,));
    tx.execute(&insert, (1,)).await?;
    panic!("boom");
}

async fn nested_savepoint_body(tx: Transaction<'_>) -> babar::Result<()> {
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO savepoint_demo (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    tx.execute(&insert, (1, "outer-before".to_string())).await?;

    let rolled_back = tx.savepoint(middle_savepoint).await;
    assert!(matches!(rolled_back, Err(Error::Config(_))));

    tx.execute(&insert, (4, "outer-after".to_string())).await?;
    Ok(())
}

async fn middle_savepoint(tx: Savepoint<'_>) -> babar::Result<()> {
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO savepoint_demo (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    tx.execute(&insert, (2, "middle-before".to_string()))
        .await?;
    tx.savepoint(inner_savepoint).await?;
    Err(Error::Config("rollback middle".into()))
}

async fn inner_savepoint(tx: Savepoint<'_>) -> babar::Result<()> {
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO savepoint_demo (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    tx.execute(&insert, (3, "inner".to_string())).await?;
    Ok(())
}
