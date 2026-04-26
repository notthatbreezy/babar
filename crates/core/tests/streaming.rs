//! M2 integration tests for portal-backed row streaming.

mod common;

use std::time::Duration;

use babar::codec::{int4, text};
use babar::query::Query;
use babar::Session;
use common::{AuthMode, PgContainer};
use futures_util::StreamExt;

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
async fn streams_large_result_sets_with_backpressure() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let q: Query<(), (i32,)> = Query::raw(
        "SELECT gs::int4 FROM generate_series(1, 10000) AS gs ORDER BY gs",
        (),
        (int4,),
    );
    let mut stream = session
        .stream_with_batch_size(&q, (), 64)
        .await
        .expect("stream");

    let mut expected = 1_i32;
    while let Some(row) = stream.next().await {
        let (value,) = row.expect("row");
        assert_eq!(value, expected);
        expected += 1;

        if value % 257 == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    assert_eq!(expected, 10_001);

    let check: Query<(), (i32,)> = Query::raw("SELECT 1::int4", (), (int4,));
    assert_eq!(
        session.query(&check, ()).await.expect("session usable"),
        vec![(1,)]
    );

    session.close().await.expect("close");
}

#[tokio::test]
async fn dropping_stream_releases_temporary_statement() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let q: Query<(), (i32,)> = Query::raw(
        "SELECT gs::int4 FROM generate_series(1, 1000) AS gs ORDER BY gs",
        (),
        (int4,),
    );
    let mut stream = session
        .stream_with_batch_size(&q, (), 8)
        .await
        .expect("stream");

    for expected in 1_i32..=10 {
        let (value,) = stream
            .next()
            .await
            .expect("row available")
            .expect("decode row");
        assert_eq!(value, expected);
    }
    drop(stream);

    tokio::time::sleep(Duration::from_millis(100)).await;

    let check_prepared: Query<(String,), (String,)> = Query::raw(
        "SELECT name FROM pg_prepared_statements WHERE name LIKE $1 ORDER BY name",
        (text,),
        (text,),
    );
    let rows = session
        .query(&check_prepared, ("babar_stream_stmt_%".to_string(),))
        .await
        .expect("check leaked statements");
    assert!(
        rows.is_empty(),
        "temporary stream statements should be cleaned up, found {rows:?}"
    );

    let check: Query<(), (i32,)> = Query::raw("SELECT 1::int4", (), (int4,));
    assert_eq!(
        session.query(&check, ()).await.expect("session usable"),
        vec![(1,)]
    );

    session.close().await.expect("close");
}
