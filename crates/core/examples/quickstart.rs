//! M1 quickstart: end-to-end exercise of the typed surface.
//!
//! Connects to a Postgres instance, creates a temporary table, inserts
//! three rows with parameterized values, runs a parameterized SELECT,
//! and prints each decoded row. Reads the same `PG*` environment
//! variables as `m0_smoke`.
//!
//! ```text
//! PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=secret \
//!     PGDATABASE=postgres cargo run --example quickstart
//! ```

use std::process::ExitCode;

use babar::codec::{bool, int4, nullable, text};
use babar::query::{Command, Query};
use babar::{Config, Session};

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
        .application_name("babar-quickstart");

    let session = match Session::connect(cfg).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("connect failed: {e}");
            return ExitCode::from(1);
        }
    };

    println!(
        "connected: server_version={}",
        session.params().get("server_version").unwrap_or("?")
    );

    if let Err(e) = run(&session).await {
        eprintln!("workflow failed: {e}");
        let _ = session.close().await;
        return ExitCode::from(1);
    }

    if let Err(e) = session.close().await {
        eprintln!("close failed: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

async fn run(session: &Session) -> babar::Result<()> {
    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE quickstart (
            id int4 PRIMARY KEY,
            name text NOT NULL,
            active bool NOT NULL,
            note text
         )",
        (),
    );
    let _ = session.execute(&create, ()).await?;
    println!("created TEMP TABLE quickstart");

    let insert: Command<(i32, String, core::primitive::bool, Option<String>)> = Command::raw(
        "INSERT INTO quickstart (id, name, active, note) VALUES ($1, $2, $3, $4)",
        (int4, text, bool, nullable(text)),
    );
    let rows = [
        (1_i32, "alice".to_string(), true, Some("first".to_string())),
        (2_i32, "bob".to_string(), false, None),
        (3_i32, "carol".to_string(), true, Some("third".to_string())),
    ];
    for row in &rows {
        let n = session.execute(&insert, row.clone()).await?;
        println!("inserted {n} row(s) for id={}", row.0);
    }

    let select: Query<
        (core::primitive::bool,),
        (i32, String, core::primitive::bool, Option<String>),
    > = Query::raw(
        "SELECT id, name, active, note FROM quickstart WHERE active = $1 ORDER BY id",
        (bool,),
        (int4, text, bool, nullable(text)),
    );
    let active_rows = session.query(&select, (true,)).await?;
    println!("active rows ({}):", active_rows.len());
    for (id, name, active, note) in &active_rows {
        let note = note.as_deref().unwrap_or("(none)");
        println!("  id={id} name={name} active={active} note={note}");
    }

    Ok(())
}
