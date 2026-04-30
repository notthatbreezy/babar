//! M3 quickstart: end-to-end exercise of the schema-aware typed SQL surface.
//!
//! Connects to a Postgres instance, creates a temporary table, inserts
//! three rows with parameterized values, runs a parameterized SELECT,
//! and prints each decoded row. Application queries use schema-scoped
//! `query!` / `command!` wrappers; the one raw fallback is the DDL setup step.
//!
//! Reads the same `PG*` environment variables as `m0_smoke`.
//!
//! ```text
//! PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=secret \
//!     PGDATABASE=postgres cargo run --example quickstart
//! ```

use std::process::ExitCode;

use babar::query::{Command, Query};
use babar::{Config, Session};

babar::schema! {
    mod quickstart_schema {
        table quickstart {
            id: primary_key(int4),
            name: text,
            active: bool,
            note: nullable(text),
        },
    }
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

    let insert: Command<(i32, String, core::primitive::bool, Option<String>)> = quickstart_schema::command!(
        INSERT INTO quickstart (id, name, active, note)
        VALUES ($id, $name, $active, $note)
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
    > = quickstart_schema::query!(
        SELECT quickstart.id, quickstart.name, quickstart.active, quickstart.note
        FROM quickstart
        WHERE quickstart.active = $active
        ORDER BY quickstart.id
    );
    let active_rows = session.query(&select, (true,)).await?;
    println!("active rows ({}):", active_rows.len());
    for (id, name, active, note) in &active_rows {
        let note = note.as_deref().unwrap_or("(none)");
        println!("  id={id} name={name} active={active} note={note}");
    }

    Ok(())
}
