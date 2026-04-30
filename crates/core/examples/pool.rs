//! M4 example: connection pooling plus pooled prepared statements.
//!
//! ```text
//! PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=secret \
//!     PGDATABASE=postgres cargo run -p babar --example pool
//! ```

use std::process::ExitCode;
use std::time::Duration;

use babar::codec::{int4, text};
use babar::query::{Command, Query};
use babar::{Config, HealthCheck, Pool, PoolConfig};

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

    let connect = Config::new(&host, port, &user, &database)
        .password(password)
        .application_name("babar-pool-example");
    let pool = match Pool::new(
        connect,
        PoolConfig::new()
            .min_idle(1)
            // This example uses a TEMP TABLE, which is scoped to one server
            // session. Keep pool size at 1 so the second checkout reuses the
            // same physical connection deterministically.
            .max_size(1)
            .acquire_timeout(Duration::from_secs(2))
            .idle_timeout(Duration::from_secs(30))
            .max_lifetime(Duration::from_secs(300))
            .health_check(HealthCheck::Ping),
    )
    .await
    {
        Ok(pool) => pool,
        Err(err) => {
            eprintln!("pool build failed: {err}");
            return ExitCode::from(1);
        }
    };

    if let Err(err) = run(&pool).await {
        eprintln!("example failed: {err}");
        pool.close().await;
        return ExitCode::from(1);
    }

    pool.close().await;
    ExitCode::SUCCESS
}

async fn run(pool: &Pool) -> babar::Result<()> {
    let conn = pool.acquire().await.map_err(pool_error)?;
    let create: Command<()> =
        Command::raw("CREATE TEMP TABLE pool_example (id int4 PRIMARY KEY, note text NOT NULL)");
    let insert: Command<(i32, String)> = Command::raw_with(
        "INSERT INTO pool_example (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    let lookup: Query<(i32,), (String,)> = Query::raw_with(
        "SELECT note FROM pool_example WHERE id = $1",
        (int4,),
        (text,),
    );

    conn.execute(&create, ()).await?;
    conn.execute(&insert, (1, "first checkout".to_string()))
        .await?;
    let prepared = conn.prepare_query(&lookup).await?;
    println!("prepared server-side statement: {}", prepared.name());
    println!("first result: {:?}", prepared.query((1,)).await?);
    drop(prepared);
    drop(conn);

    let conn = pool.acquire().await.map_err(pool_error)?;
    let prepared = conn.prepare_query(&lookup).await?;
    println!("reused server-side statement: {}", prepared.name());
    println!("second result: {:?}", prepared.query((1,)).await?);
    drop(prepared);
    drop(conn);

    Ok(())
}

fn pool_error(err: babar::PoolError) -> babar::Error {
    match err {
        babar::PoolError::AcquireFailed(inner) => inner,
        other => babar::Error::Config(other.to_string()),
    }
}
