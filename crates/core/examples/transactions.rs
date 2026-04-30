//! M4 example: scoped transactions with nested savepoints.
//!
//! ```text
//! PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=secret \
//!     PGDATABASE=postgres cargo run -p babar --example transactions
//! ```

use std::process::ExitCode;

use babar::codec::{int4, text};
use babar::query::{Command, Query};
use babar::{Config, Error, Savepoint, Session, Transaction};

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
        .application_name("babar-transactions-example");

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
    let create: Command<()> =
        Command::raw("CREATE TEMP TABLE tx_example (id int4 PRIMARY KEY, note text NOT NULL)");
    let select: Query<(), (i32, String)> =
        Query::raw("SELECT id, note FROM tx_example ORDER BY id", (int4, text));
    session.execute(&create, ()).await?;

    session.transaction(transaction_body).await?;

    println!("rows committed after outer transaction:");
    for (id, note) in session.query(&select, ()).await? {
        println!("  {id}: {note}");
    }
    Ok(())
}

async fn transaction_body(tx: Transaction<'_>) -> babar::Result<()> {
    let insert: Command<(i32, String)> = Command::raw_with(
        "INSERT INTO tx_example (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    tx.execute(&insert, (1, "outer-before".to_string())).await?;
    let middle = tx.savepoint(rollbacking_savepoint).await;
    assert!(matches!(middle, Err(Error::Config(_))));
    tx.execute(&insert, (3, "outer-after".to_string())).await?;
    Ok(())
}

async fn rollbacking_savepoint(sp: Savepoint<'_>) -> babar::Result<()> {
    let insert: Command<(i32, String)> = Command::raw_with(
        "INSERT INTO tx_example (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    sp.execute(&insert, (2, "savepoint".to_string())).await?;
    Err(Error::Config("rolling back inner savepoint".into()))
}
