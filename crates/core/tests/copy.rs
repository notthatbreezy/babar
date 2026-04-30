//! Integration coverage for typed binary `COPY FROM STDIN`.

mod common;

use babar::codec::int8;
use babar::query::Query;
use babar::{CopyIn, Error, Session};
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

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct CopyUser {
    #[pg(codec = "int4")]
    id: i32,
    #[pg(codec = "text")]
    name: String,
    #[pg(codec = "bool")]
    active: bool,
    #[pg(codec = "nullable(text)")]
    note: Option<String>,
    #[pg(codec = "int8")]
    visits: i64,
}

async fn create_copy_users(session: &Session, extra_constraints: &str) {
    session
        .simple_query_raw(&format!(
            "CREATE TEMP TABLE copy_users (\
                id int4 PRIMARY KEY,\
                name text NOT NULL,\
                active bool NOT NULL,\
                note text,\
                visits int8 NOT NULL\
                {extra_constraints}\
            )"
        ))
        .await
        .expect("create table");
}

fn copy_users_statement() -> CopyIn<CopyUser> {
    CopyIn::binary(
        "COPY copy_users (id, name, active, note, visits) FROM STDIN BINARY",
        CopyUser::CODEC,
    )
}

async fn select_copy_users(session: &Session) -> Vec<CopyUser> {
    let select: Query<(), CopyUser> = Query::raw(
        "SELECT id, name, active, note, visits FROM copy_users ORDER BY id",
        CopyUser::CODEC,
    );
    session.query(&select, ()).await.expect("select rows")
}

#[tokio::test]
async fn typed_copy_in_bulk_inserts_vec_of_struct_rows() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    create_copy_users(&session, "").await;

    let expected = vec![
        CopyUser {
            id: 1,
            name: "alice".into(),
            active: true,
            note: Some("beta tester".into()),
            visits: 4,
        },
        CopyUser {
            id: 2,
            name: "bob".into(),
            active: false,
            note: Some("nightly".into()),
            visits: 9,
        },
    ];

    let affected = session
        .copy_in(&copy_users_statement(), expected.clone())
        .await
        .expect("copy rows");
    assert_eq!(affected, expected.len() as u64);

    assert_eq!(select_copy_users(&session).await, expected);

    session.close().await.expect("close");
}

#[tokio::test]
async fn typed_copy_in_preserves_null_fields() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    create_copy_users(&session, "").await;

    let expected = vec![
        CopyUser {
            id: 1,
            name: "alice".into(),
            active: true,
            note: None,
            visits: 4,
        },
        CopyUser {
            id: 2,
            name: "bob".into(),
            active: false,
            note: Some("present".into()),
            visits: 9,
        },
        CopyUser {
            id: 3,
            name: "cara".into(),
            active: true,
            note: None,
            visits: 11,
        },
    ];

    let affected = session
        .copy_in(&copy_users_statement(), expected.clone())
        .await
        .expect("copy rows");
    assert_eq!(affected, expected.len() as u64);

    assert_eq!(select_copy_users(&session).await, expected);

    session.close().await.expect("close");
}

#[tokio::test]
async fn typed_copy_in_constraint_failure_recovers_connection() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    create_copy_users(
        &session,
        ", CONSTRAINT visits_nonnegative CHECK (visits >= 0)",
    )
    .await;

    let bad_rows = vec![
        CopyUser {
            id: 1,
            name: "alice".into(),
            active: true,
            note: Some("ok".into()),
            visits: 4,
        },
        CopyUser {
            id: 2,
            name: "bob".into(),
            active: false,
            note: None,
            visits: -1,
        },
    ];

    match session
        .copy_in(&copy_users_statement(), bad_rows)
        .await
        .expect_err("constraint violation")
    {
        Error::Server { code, message, .. } => {
            assert_eq!(code, "23514");
            assert!(message.contains("violates check constraint"));
        }
        other => panic!("expected Error::Server, got {other:?}"),
    }

    let count: Query<(), (i64,)> = Query::raw("SELECT count(*)::int8 FROM copy_users", (int8,));
    assert_eq!(
        session.query(&count, ()).await.expect("count rows"),
        vec![(0,)]
    );

    let recovered_rows = vec![
        CopyUser {
            id: 3,
            name: "cara".into(),
            active: true,
            note: Some("recovered".into()),
            visits: 12,
        },
        CopyUser {
            id: 4,
            name: "dave".into(),
            active: false,
            note: None,
            visits: 2,
        },
    ];

    let affected = session
        .copy_in(&copy_users_statement(), recovered_rows.clone())
        .await
        .expect("copy after failure");
    assert_eq!(affected, recovered_rows.len() as u64);
    assert_eq!(select_copy_users(&session).await, recovered_rows);

    session.close().await.expect("close");
}
