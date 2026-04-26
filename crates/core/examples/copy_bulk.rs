//! Bulk ingest with typed binary `COPY FROM STDIN`.
//!
//! ```text
//! PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=secret \
//!     PGDATABASE=postgres cargo run -p babar --example copy_bulk
//! ```

use std::process::ExitCode;

use babar::query::Query;
use babar::{Config, CopyIn, Session};

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct VisitRow {
    id: i32,
    email: String,
    active: bool,
    note: Option<String>,
    visits: i64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let host = std::env::var("PGHOST").unwrap_or_else(|_| "localhost".into());
    let port: u16 = std::env::var("PGPORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5432);
    let user = std::env::var("PGUSER").unwrap_or_else(|_| "postgres".into());
    let database = std::env::var("PGDATABASE").unwrap_or_else(|_| user.clone());
    let password = std::env::var("PGPASSWORD").unwrap_or_else(|_| "postgres".into());

    let cfg = Config::new(&host, port, &user, &database)
        .password(password)
        .application_name("babar-copy-bulk");

    let session = match Session::connect(cfg).await {
        Ok(session) => session,
        Err(err) => {
            eprintln!("connect failed: {err}");
            return ExitCode::from(1);
        }
    };

    if let Err(err) = run(&session).await {
        eprintln!("example failed: {err}");
        let _ = session.close().await;
        return ExitCode::from(1);
    }

    if let Err(err) = session.close().await {
        eprintln!("close failed: {err}");
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

async fn run(session: &Session) -> babar::Result<()> {
    session
        .simple_query_raw(
            "CREATE TEMP TABLE bulk_visits (\
                id int4 PRIMARY KEY,\
                email text NOT NULL,\
                active bool NOT NULL,\
                note text,\
                visits int8 NOT NULL\
            )",
        )
        .await?;

    let rows = vec![
        VisitRow {
            id: 1,
            email: "ada@example.com".into(),
            active: true,
            note: Some("first import".into()),
            visits: 7,
        },
        VisitRow {
            id: 2,
            email: "bob@example.com".into(),
            active: false,
            note: None,
            visits: 3,
        },
        VisitRow {
            id: 3,
            email: "cara@example.com".into(),
            active: true,
            note: Some("newsletter".into()),
            visits: 12,
        },
    ];

    let copy: CopyIn<VisitRow> = CopyIn::binary(
        "COPY bulk_visits (id, email, active, note, visits) FROM STDIN BINARY",
        VisitRow::CODEC,
    );
    let affected = session.copy_in(&copy, rows.clone()).await?;
    println!("copied {affected} rows");

    let select: Query<(), VisitRow> = Query::raw(
        "SELECT id, email, active, note, visits FROM bulk_visits ORDER BY id",
        (),
        VisitRow::CODEC,
    );
    for row in session.query(&select, ()).await? {
        println!("{row:?}");
    }

    Ok(())
}
