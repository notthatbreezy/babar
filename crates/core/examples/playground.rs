//! Hands-on playground for the babar surface.
//!
//! Run it with a Postgres pointed at by the standard `PG*` env vars (or the
//! defaults below).

use std::time::{Duration, Instant};

use babar::{Config, Error, Session};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "babar=info".to_string()))
        .with_target(false)
        .try_init()
        .ok();
    run_playground().await
}

async fn run_playground() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cfg = config_from_env();

    println!("=== 1. Connect ===");
    let session = section_connect(cfg.clone()).await?;
    println!();

    println!("=== 2. Server parameters reported during startup ===");
    section_server_params(&session);
    println!();

    println!("=== 3. Single SELECT — text rows ===");
    section_select_one(&session).await?;
    println!();

    println!("=== 4. Multi-statement simple query ===");
    section_multi_statement(&session).await?;
    println!();

    println!("=== 5. NULLs and mixed columns ===");
    section_nulls(&session).await?;
    println!();

    println!("=== 6. Server-side errors round-trip cleanly ===");
    section_server_error(&session).await;
    println!();

    println!("=== 7. Concurrent queries on one Session ===");
    let session = section_concurrent(session).await?;
    println!();

    println!("=== 8. Wrong password produces Error::Auth ===");
    section_wrong_password(cfg.clone()).await;
    println!();

    println!("=== 9. Clean close ===");
    session.close().await?;
    println!("closed.");

    Ok(())
}

fn config_from_env() -> Config {
    let host = std::env::var("PGHOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = std::env::var("PGPORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(54320);
    let user = std::env::var("PGUSER").unwrap_or_else(|_| "babar".into());
    let database = std::env::var("PGDATABASE").unwrap_or_else(|_| user.clone());
    let password = std::env::var("PGPASSWORD").unwrap_or_else(|_| "secret".into());

    Config::new(&host, port, &user, &database)
        .password(password)
        .application_name("babar-playground")
        .connect_timeout(Duration::from_secs(5))
}

async fn section_connect(cfg: Config) -> Result<Session, Error> {
    let started = Instant::now();
    let session = Session::connect(cfg).await?;
    println!("connected in {:?}", started.elapsed());
    if let Some((pid, _)) = session.backend_key() {
        println!("backend pid: {pid}");
    }
    Ok(session)
}

fn section_server_params(session: &Session) {
    let interesting = [
        "server_version",
        "server_encoding",
        "client_encoding",
        "TimeZone",
        "integer_datetimes",
        "is_superuser",
    ];
    for name in interesting {
        match session.params().get(name) {
            Some(value) => println!("  {name:>20} = {value}"),
            None => println!("  {name:>20} = <not reported>"),
        }
    }
}

async fn section_select_one(session: &Session) -> Result<(), Error> {
    let result_sets = session.simple_query_raw("SELECT 1, 'hello', now()").await?;
    print_result_sets(&result_sets);
    Ok(())
}

async fn section_multi_statement(session: &Session) -> Result<(), Error> {
    let sql = "SELECT 1 AS one; SELECT 'two'; SELECT generate_series(1, 3) AS n";
    let result_sets = session.simple_query_raw(sql).await?;
    print_result_sets(&result_sets);
    Ok(())
}

async fn section_nulls(session: &Session) -> Result<(), Error> {
    let sql = "SELECT NULL::text AS missing, 'present' AS here, 42 AS answer";
    let result_sets = session.simple_query_raw(sql).await?;
    print_result_sets(&result_sets);
    Ok(())
}

async fn section_server_error(session: &Session) {
    match session
        .simple_query_raw("SELECT * FROM no_such_table")
        .await
    {
        Err(Error::Server {
            code,
            severity,
            message,
            ..
        }) => {
            println!("got expected server error:");
            println!("  severity = {severity}");
            println!("  sqlstate = {code}");
            println!("  message  = {message}");
        }
        Err(other) => println!("unexpected error type: {other}"),
        Ok(_) => println!("(unexpectedly succeeded — does the table exist?)"),
    }
}

async fn section_concurrent(session: Session) -> Result<Session, Error> {
    use std::sync::Arc;

    let session = Arc::new(session);
    let n = 16;
    let started = Instant::now();
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        let s = Arc::clone(&session);
        handles.push(tokio::spawn(async move {
            let sql = format!("SELECT {i}, pg_sleep(0.05)");
            s.simple_query_raw(&sql).await.map(|rs| (i, rs))
        }));
    }
    let mut completed = 0;
    for h in handles {
        let (i, rs) = h.await.expect("task panicked")?;
        let echoed = rs[0][0][0]
            .as_deref()
            .and_then(|b| std::str::from_utf8(b).ok());
        println!("  task {i:>2} echoed back {echoed:?}");
        completed += 1;
    }
    println!(
        "ran {completed} concurrent queries in {:?}",
        started.elapsed()
    );

    Ok(Arc::try_unwrap(session).expect("all task handles dropped"))
}

async fn section_wrong_password(mut cfg: Config) {
    cfg = cfg.password("definitely-not-the-password");
    match Session::connect(cfg).await {
        Err(Error::Auth(msg)) => println!("Error::Auth as expected: {msg}"),
        Err(Error::Server { code, message, .. }) => {
            println!("Error::Server (some servers report this instead): {code} {message}");
        }
        Err(other) => println!("unexpected error variant: {other}"),
        Ok(_) => println!("(unexpectedly succeeded — is the server in trust mode?)"),
    }
}

fn print_result_sets(result_sets: &[Vec<Vec<Option<bytes::Bytes>>>]) {
    for (i, rows) in result_sets.iter().enumerate() {
        println!("  result set #{i}: {} row(s)", rows.len());
        for (r, row) in rows.iter().enumerate() {
            let cells: Vec<String> = row
                .iter()
                .map(|c| match c {
                    Some(b) => format!("\"{}\"", String::from_utf8_lossy(b)),
                    None => "NULL".into(),
                })
                .collect();
            println!("    row {r}: [{}]", cells.join(", "));
        }
    }
}
