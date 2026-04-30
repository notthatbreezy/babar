//! M2 example: prepared statements plus portal-backed streaming.
//!
//! Reads the standard `PG*` environment variables, creates a temporary table,
//! prepares one command and one query, executes the prepared query multiple
//! times, then streams the full result set in small batches.
//!
//! ```text
//! PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=secret \
//!     PGDATABASE=postgres cargo run -p babar --example prepared_and_stream
//! ```

use std::process::ExitCode;

use babar::codec::{int4, text};
use babar::query::{Command, Query};
use babar::{Config, Session};
use futures_util::StreamExt;

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
        .application_name("babar-prepared-and-stream");

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
        "CREATE TEMP TABLE prepared_demo (
            id int4 PRIMARY KEY,
            title text NOT NULL
         )",
    );
    session.execute(&create, ()).await?;

    let insert: Command<(i32, String)> = Command::raw_with(
        "INSERT INTO prepared_demo (id, title) VALUES ($1, $2)",
        (int4, text),
    );
    let prepared_insert = session.prepare_command(&insert).await?;
    for (id, title) in [
        (1_i32, "alpha"),
        (2_i32, "beta"),
        (3_i32, "gamma"),
        (4_i32, "delta"),
        (5_i32, "epsilon"),
    ] {
        let affected = prepared_insert.execute((id, title.to_string())).await?;
        println!("inserted {affected} row(s) for id={id}");
    }
    prepared_insert.close().await?;

    let lookup: Query<(i32,), (i32, String)> = Query::raw_with(
        "SELECT id, title FROM prepared_demo WHERE id >= $1 ORDER BY id",
        (int4,),
        (int4, text),
    );
    let prepared_lookup = session.prepare_query(&lookup).await?;
    println!(
        "prepared lookup as server statement {}",
        prepared_lookup.name()
    );

    for lower_bound in [2_i32, 4_i32] {
        let rows = prepared_lookup.query((lower_bound,)).await?;
        println!("rows with id >= {lower_bound}:");
        for (id, title) in rows {
            println!("  {id}: {title}");
        }
    }
    prepared_lookup.close().await?;

    let stream_query: Query<(), (i32, String)> = Query::raw(
        "SELECT id, title FROM prepared_demo ORDER BY id",
        (int4, text),
    );
    let mut rows = session.stream_with_batch_size(&stream_query, (), 2).await?;
    println!("streaming full result set in batches of 2:");
    while let Some(row) = rows.next().await {
        let (id, title) = row?;
        println!("  streamed {id}: {title}");
    }

    Ok(())
}
