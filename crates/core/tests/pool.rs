//! M4 integration tests for connection pooling.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use babar::codec::{bool, int4, int8, nullable, text};
use babar::query::{Command, Query};
use babar::{Error, HealthCheck, Pool, PoolConfig, PooledSavepoint, PooledTransaction, Session};
use common::{AuthMode, PgContainer};
use futures_util::FutureExt;

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

async fn admin_session(pg: &PgContainer, application_name: &str) -> Session {
    Session::connect(
        pg.config(pg.user(), pg.password())
            .application_name(application_name),
    )
    .await
    .expect("connect admin session")
}

async fn terminate_backend(pg: &PgContainer, pid: i32) {
    let admin = admin_session(pg, "babar-m4-admin").await;
    let terminate: Query<(i32,), (bool,)> =
        Query::raw("SELECT pg_terminate_backend($1)", (int4,), (bool,));
    let rows = admin
        .query(&terminate, (pid,))
        .await
        .expect("terminate backend");
    assert_eq!(rows, vec![(true,)]);
    admin.close().await.expect("close admin session");
}

async fn fresh_pool(
    pool_config: PoolConfig,
    application_name: &str,
) -> Option<(PgContainer, Pool)> {
    if !require_docker() {
        return None;
    }
    let pg = PgContainer::start(AuthMode::Scram).await;
    let pool = Pool::new(
        pg.config(pg.user(), pg.password())
            .application_name(application_name),
        pool_config,
    )
    .await
    .expect("build pool");
    Some((pg, pool))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pool_handles_concurrent_load_without_deadlock() {
    let Some((_pg, pool)) = fresh_pool(
        PoolConfig::new()
            .max_size(10)
            .min_idle(2)
            .acquire_timeout(Duration::from_secs(5))
            .health_check(HealthCheck::Ping),
        "babar-m4-pool-concurrency",
    )
    .await
    else {
        return;
    };

    let pool = Arc::new(pool);
    let query: Query<(i32,), (i32,)> =
        Query::raw("SELECT $1::int4 FROM pg_sleep(0.01)", (int4,), (int4,));

    let started = Instant::now();
    let mut tasks = Vec::new();
    for i in 0..100_i32 {
        let pool = Arc::clone(&pool);
        let query = Query::raw(query.sql().to_string(), (int4,), (int4,));
        tasks.push(tokio::spawn(async move {
            let conn = pool.acquire().await.expect("acquire pooled connection");
            let rows = conn.query(&query, (i,)).await.expect("run query");
            assert_eq!(rows, vec![(i,)]);
        }));
    }
    for task in tasks {
        task.await.expect("join task");
    }
    assert!(started.elapsed() < Duration::from_secs(5));

    pool.close().await;
}

#[tokio::test]
async fn acquire_replaces_dead_idle_connection() {
    let Some((pg, pool)) = fresh_pool(
        PoolConfig::new()
            .max_size(1)
            .min_idle(1)
            .acquire_timeout(Duration::from_secs(5))
            .health_check(HealthCheck::Ping),
        "babar-m4-pool-kill",
    )
    .await
    else {
        return;
    };

    let first = pool.acquire().await.expect("acquire first connection");
    let first_pid = first.backend_key().expect("backend key available").0;
    drop(first);
    tokio::time::sleep(Duration::from_millis(100)).await;

    terminate_backend(&pg, first_pid).await;

    let second = pool
        .acquire()
        .await
        .expect("acquire replacement connection");
    let second_pid = second.backend_key().expect("backend key available").0;
    let ping: Query<(), (i32,)> = Query::raw("SELECT 1::int4", (), (int4,));
    assert_eq!(
        second.query(&ping, ()).await.expect("query replacement"),
        vec![(1,)]
    );
    assert_ne!(first_pid, second_pid, "dead connection should be replaced");

    drop(second);
    pool.close().await;
}

#[tokio::test]
async fn transaction_panic_returns_clean_connection_to_pool() {
    let Some((_pg, pool)) = fresh_pool(
        PoolConfig::new()
            .max_size(1)
            .min_idle(0)
            .acquire_timeout(Duration::from_secs(5))
            .health_check(HealthCheck::Ping),
        "babar-m4-pool-panic",
    )
    .await
    else {
        return;
    };

    let conn = pool.acquire().await.expect("acquire connection");
    let create: Command<()> = Command::raw("CREATE TEMP TABLE pooled_panic_demo (id int4)", ());
    conn.execute(&create, ()).await.expect("create temp table");

    let panic_result = std::panic::AssertUnwindSafe(conn.transaction(pooled_panic_body))
        .catch_unwind()
        .await;
    assert!(panic_result.is_err());
    drop(conn);

    tokio::time::sleep(Duration::from_millis(100)).await;

    let conn = pool.acquire().await.expect("reacquire connection");
    let txid: Query<(), (Option<i64>,)> = Query::raw(
        "SELECT txid_current_if_assigned()::int8",
        (),
        (nullable(int8),),
    );
    assert_eq!(
        conn.query(&txid, ()).await.expect("txid check"),
        vec![(None,)]
    );

    drop(conn);
    pool.close().await;
}

#[tokio::test]
async fn nested_savepoints_and_prepared_cache_survive_checkout_return_checkout() {
    let Some((_pg, pool)) = fresh_pool(
        PoolConfig::new()
            .max_size(1)
            .min_idle(0)
            .acquire_timeout(Duration::from_secs(5))
            .health_check(HealthCheck::None),
        "babar-m4-pool-prepare",
    )
    .await
    else {
        return;
    };

    let conn = pool.acquire().await.expect("acquire connection");
    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE pool_prepare_demo (id int4 PRIMARY KEY, note text)",
        (),
    );
    conn.execute(&create, ()).await.expect("create temp table");
    conn.transaction(pooled_savepoint_body)
        .await
        .expect("transaction succeeds");

    let select: Query<(i32,), (String,)> = Query::raw(
        "SELECT note FROM pool_prepare_demo WHERE id = $1",
        (int4,),
        (text,),
    );
    let prepared = conn.prepare_query(&select).await.expect("prepare query");
    let name = prepared.name().to_string();
    assert_eq!(
        prepared.query((1,)).await.expect("prepared query"),
        vec![("outer".to_string(),)]
    );
    drop(prepared);
    drop(conn);

    tokio::time::sleep(Duration::from_millis(100)).await;

    let conn = pool.acquire().await.expect("reacquire connection");
    let prepared = conn
        .prepare_query(&select)
        .await
        .expect("prepare query again");
    assert_eq!(prepared.name(), name);
    assert_eq!(
        prepared.query((1,)).await.expect("prepared query again"),
        vec![("outer".to_string(),)]
    );
    drop(prepared);
    drop(conn);
    pool.close().await;
}

async fn pooled_panic_body(tx: PooledTransaction<'_>) -> babar::Result<()> {
    let insert: Command<(i32,)> =
        Command::raw("INSERT INTO pooled_panic_demo (id) VALUES ($1)", (int4,));
    tx.execute(&insert, (1,)).await?;
    panic!("panic inside pooled transaction");
}

async fn pooled_savepoint_body(tx: PooledTransaction<'_>) -> babar::Result<()> {
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO pool_prepare_demo (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    tx.execute(&insert, (1, "outer".to_string())).await?;
    let rolled_back = tx.savepoint(pooled_rollback_savepoint).await;
    assert!(matches!(rolled_back, Err(Error::Config(_))));
    Ok(())
}

async fn pooled_rollback_savepoint(tx: PooledSavepoint<'_>) -> babar::Result<()> {
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO pool_prepare_demo (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    tx.execute(&insert, (2, "inner".to_string())).await?;
    Err(Error::Config("rollback savepoint".into()))
}

#[tokio::test]
async fn connection_error_during_transaction_evicts_connection() {
    let Some((pg, pool)) = fresh_pool(
        PoolConfig::new()
            .max_size(1)
            .min_idle(0)
            .acquire_timeout(Duration::from_secs(5))
            .health_check(HealthCheck::Ping),
        "babar-m4-pool-evict",
    )
    .await
    else {
        return;
    };

    let conn = pool.acquire().await.expect("acquire connection");
    let pid = conn.backend_key().expect("backend key available").0;
    let begin_insert: Command<(i32,)> = Command::raw("SELECT pg_sleep(0.1), $1::int4", (int4,));

    terminate_backend(&pg, pid).await;

    let err = conn
        .execute(&begin_insert, (1,))
        .await
        .expect_err("query should fail after backend termination");
    assert!(matches!(
        err,
        Error::Closed { .. } | Error::Server { .. } | Error::Io(_)
    ));
    drop(conn);

    tokio::time::sleep(Duration::from_millis(100)).await;

    let conn = pool
        .acquire()
        .await
        .expect("acquire replacement connection");
    let ping: Query<(), (i32,)> = Query::raw("SELECT 1::int4", (), (int4,));
    assert_eq!(
        conn.query(&ping, ()).await.expect("ping replacement"),
        vec![(1,)]
    );
    drop(conn);
    pool.close().await;
}
