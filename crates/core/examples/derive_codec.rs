//! M5 example: derive a struct codec and use it for both inserts and selects.
//!
//! ```text
//! PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=secret \
//!     PGDATABASE=postgres cargo run -p babar --example derive_codec
//! ```

use std::process::ExitCode;

use babar::query::{Command, Query};
use babar::{Config, Session};

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct UserRow {
    id: i32,
    name: String,
    active: bool,
    note: Option<String>,
    visits: i64,
    #[pg(codec = "varchar")]
    handle: String,
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
        .application_name("babar-derive-codec");

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
    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE derive_codec_demo (\
            id int4 PRIMARY KEY,\
            name text NOT NULL,\
            active bool NOT NULL,\
            note text,\
            visits int8 NOT NULL,\
            handle varchar NOT NULL\
        )",
    );
    session.execute(&create, ()).await?;

    let insert: Command<UserRow> = Command::raw_with(
        "INSERT INTO derive_codec_demo (id, name, active, note, visits, handle) VALUES ($1, $2, $3, $4, $5, $6)",
        UserRow::CODEC);
    for row in [
        UserRow {
            id: 1,
            name: "alice".into(),
            active: true,
            note: Some("beta tester".into()),
            visits: 3,
            handle: "alice".into(),
        },
        UserRow {
            id: 2,
            name: "bob".into(),
            active: false,
            note: None,
            visits: 8,
            handle: "bob".into(),
        },
    ] {
        session.execute(&insert, row).await?;
    }

    let select: Query<(), UserRow> = Query::raw(
        "SELECT id, name, active, note, visits, handle FROM derive_codec_demo ORDER BY id",
        UserRow::CODEC,
    );
    let rows = session.query(&select, ()).await?;
    for row in rows {
        println!("{row:?}");
    }

    Ok(())
}
