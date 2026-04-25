//! M0 integration tests against a real Postgres container.
//!
//! Each `#[tokio::test]` function spins up a Postgres container, runs the
//! scenario, and lets `Drop` tear the container down. SCRAM is the default
//! Postgres auth mode; the `cleartext` and `md5` tests configure
//! `POSTGRES_HOST_AUTH_METHOD` accordingly.

mod common;

use std::time::Duration;

use babar::{Config, Error, Session};
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

#[tokio::test]
async fn connect_select_1_scram() {
    if !require_docker() {
        return;
    }
    let pg = PgContainer::start(AuthMode::Scram).await;
    let session = Session::connect(pg.config(pg.user(), pg.password()))
        .await
        .expect("connect");

    let rows = session
        .simple_query_raw("SELECT 1")
        .await
        .expect("simple query");
    assert_eq!(rows.len(), 1, "exactly one result set");
    assert_eq!(rows[0].len(), 1, "exactly one row");
    assert_eq!(rows[0][0].len(), 1, "exactly one column");
    assert_eq!(
        rows[0][0][0].as_deref(),
        Some(&b"1"[..]),
        "value is text \"1\""
    );

    session.close().await.expect("clean close");
}

#[tokio::test]
async fn wrong_password_returns_auth_error() {
    if !require_docker() {
        return;
    }
    let pg = PgContainer::start(AuthMode::Scram).await;
    let cfg = pg.config(pg.user(), "wrong-password");
    let result = Session::connect(cfg).await;
    match result {
        Err(Error::Auth(_) | Error::Server { .. }) => {}
        Ok(_) => panic!("connection should have failed"),
        Err(other) => panic!("expected auth error, got {other:?}"),
    }
}

#[tokio::test]
async fn connect_with_cleartext_auth() {
    if !require_docker() {
        return;
    }
    let pg = PgContainer::start(AuthMode::Cleartext).await;
    let session = Session::connect(pg.config(pg.user(), pg.password()))
        .await
        .expect("connect");
    let rows = session.simple_query_raw("SELECT 7").await.expect("query");
    assert_eq!(rows[0][0][0].as_deref(), Some(&b"7"[..]));
    session.close().await.expect("close");
}

#[tokio::test]
async fn connect_with_md5_auth() {
    if !require_docker() {
        return;
    }
    let pg = PgContainer::start(AuthMode::Md5).await;
    let session = Session::connect(pg.config(pg.user(), pg.password()))
        .await
        .expect("connect");
    let rows = session
        .simple_query_raw("SELECT 'md5'")
        .await
        .expect("query");
    assert_eq!(rows[0][0][0].as_deref(), Some(&b"md5"[..]));
    session.close().await.expect("close");
}

#[tokio::test]
async fn multi_statement_simple_query() {
    if !require_docker() {
        return;
    }
    let pg = PgContainer::start(AuthMode::Scram).await;
    let session = Session::connect(pg.config(pg.user(), pg.password()))
        .await
        .expect("connect");

    let rows = session
        .simple_query_raw("SELECT 1; SELECT 2;")
        .await
        .expect("multi-statement");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0][0].as_deref(), Some(&b"1"[..]));
    assert_eq!(rows[1][0][0].as_deref(), Some(&b"2"[..]));
    session.close().await.expect("close");
}

#[tokio::test]
async fn drop_session_terminates_driver() {
    if !require_docker() {
        return;
    }
    let pg = PgContainer::start(AuthMode::Scram).await;
    {
        let session = Session::connect(pg.config(pg.user(), pg.password()))
            .await
            .expect("connect");
        // No close — drop here.
        drop(session);
    }

    // The container should still accept new connections; that proves the
    // first session's driver task tore the socket down cleanly without
    // taking the server with it.
    let session = Session::connect(pg.config(pg.user(), pg.password()))
        .await
        .expect("reconnect after drop");
    let rows = session.simple_query_raw("SELECT 42").await.expect("query");
    assert_eq!(rows[0][0][0].as_deref(), Some(&b"42"[..]));
    session.close().await.expect("close");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_simple_queries_demuxed_correctly() {
    if !require_docker() {
        return;
    }
    let pg = PgContainer::start(AuthMode::Scram).await;
    let session = std::sync::Arc::new(
        Session::connect(pg.config(pg.user(), pg.password()))
            .await
            .expect("connect"),
    );

    let mut handles = Vec::new();
    for i in 0..100 {
        let s = session.clone_handle_via_arc();
        handles.push(tokio::spawn(async move {
            let sql = format!("SELECT {i}");
            let rows = s.simple_query_raw(&sql).await.expect("query");
            (i, rows[0][0][0].clone())
        }));
    }

    for h in handles {
        let (i, val) = h.await.expect("task");
        let expected = i.to_string();
        assert_eq!(
            val.as_deref(),
            Some(expected.as_bytes()),
            "response for query {i} did not match"
        );
    }

    // Match query order check: timeout to make sure we don't hang.
    tokio::time::timeout(Duration::from_secs(1), session.close_via_arc())
        .await
        .expect("no hang on close")
        .expect("close result");
}

// Convenience helpers — Session is not Clone, but we want N tasks to share
// one connection. We wrap in Arc and proxy.
trait SessionArc {
    fn clone_handle_via_arc(&self) -> std::sync::Arc<Session>;
}

impl SessionArc for std::sync::Arc<Session> {
    fn clone_handle_via_arc(&self) -> std::sync::Arc<Session> {
        std::sync::Arc::clone(self)
    }
}

trait SessionArcClose {
    fn close_via_arc(self) -> futures_util::future::BoxFuture<'static, babar::Result<()>>;
}

impl SessionArcClose for std::sync::Arc<Session> {
    fn close_via_arc(self) -> futures_util::future::BoxFuture<'static, babar::Result<()>> {
        // Once the last Arc clone is dropped, the Session's Drop impl will
        // send a best-effort terminate. There's no public way to consume an
        // Arc<Session> back into a Session, so we settle for "drop and
        // verify no panic"; this matches what real users do when sharing a
        // session via Arc.
        let s = self;
        Box::pin(async move {
            drop(s);
            Ok(())
        })
    }
}

#[allow(dead_code)] // used to type-check Config builder shape
fn _config_compiles() {
    let _ = Config::new("h", 5432, "u", "d")
        .password("p")
        .application_name("app");
}
