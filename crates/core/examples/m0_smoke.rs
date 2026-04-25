//! M0 smoke test: connect, run `SELECT 1`, print the result, exit 0.
//!
//! Reads connection settings from the standard `PG*` environment
//! variables and falls back to localhost / `postgres` defaults so the
//! example is one `cargo run` away on a developer's box.
//!
//! ```text
//! PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=secret \
//!     PGDATABASE=postgres cargo run --example m0_smoke
//! ```

use std::process::ExitCode;

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
        .application_name("babar-m0-smoke");

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

    match session.simple_query_raw("SELECT 1").await {
        Ok(rows) => {
            for (i, set) in rows.iter().enumerate() {
                for row in set {
                    let cells: Vec<String> = row
                        .iter()
                        .map(|c| match c {
                            Some(b) => String::from_utf8_lossy(b).into_owned(),
                            None => "NULL".into(),
                        })
                        .collect();
                    println!("rs#{i}: [{}]", cells.join(", "));
                }
            }
        }
        Err(e) => {
            eprintln!("query failed: {e}");
            return ExitCode::from(1);
        }
    }

    if let Err(e) = session.close().await {
        eprintln!("close failed: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
