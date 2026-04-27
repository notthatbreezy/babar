# 6. Pooling

In this chapter we'll trade `Session::connect` for a `Pool` of warm
connections, discuss the knobs that matter, and see how prepared
statements live alongside pooled connections.

## Setup

```rust
use std::time::Duration;

use babar::codec::{int4, text};
use babar::query::{Command, Query};
use babar::{Config, HealthCheck, Pool, PoolConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() -> babar::Result<()> {
    let connect = Config::new("localhost", 5432, "postgres", "postgres")
        .password("postgres")
        .application_name("ch06-pool");

    let pool: Pool = Pool::new(                                       // type: Pool
        connect,
        PoolConfig::new()
            .min_idle(2)
            .max_size(8)
            .acquire_timeout(Duration::from_secs(2))
            .idle_timeout(Duration::from_secs(30))
            .max_lifetime(Duration::from_secs(300))
            .health_check(HealthCheck::Ping),
    )
    .await?;

    // Each acquire() hands you a connection scoped to the binding.
    let conn = pool.acquire().await?;                                 // type: PoolConnection
    let create: Command<()> = Command::raw(
        "CREATE TEMP TABLE pool_demo (id int4 PRIMARY KEY, note text NOT NULL)",
        (),
    );
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO pool_demo (id, note) VALUES ($1, $2)",
        (int4, text),
    );
    let lookup: Query<(i32,), (String,)> = Query::raw(
        "SELECT note FROM pool_demo WHERE id = $1",
        (int4,),
        (text,),
    );

    conn.execute(&create, ()).await?;
    conn.execute(&insert, (1, "first checkout".into())).await?;

    let prepared = conn.prepare_query(&lookup).await?;
    println!("prepared on server as: {}", prepared.name());
    println!("{:?}", prepared.query((1,)).await?);

    drop(prepared);
    drop(conn);  // returns the connection to the pool

    pool.close().await;
    Ok(())
}
```

## What a pool gives you

`Pool::new(config, pool_config)` opens up to `max_size` background
connections, keeping at least `min_idle` warm and ready. `pool.acquire()`
hands you a `PoolConnection` that behaves like a `Session` —
`execute`, `query`, `prepare_command`, `prepare_query`,
`stream_with_batch_size`, `transaction`, all of it.

Drop the `PoolConnection` and the pool reclaims it. Drop the
`Pool` itself and outstanding handles continue working until they're
dropped, at which point the connections are closed.

## The knobs that matter

| Field | What it controls |
|---|---|
| `min_idle` | Minimum number of warm connections kept open. |
| `max_size` | Hard ceiling on simultaneous connections (idle + in-use). |
| `acquire_timeout` | How long `pool.acquire()` waits before returning `PoolError::Timeout`. |
| `idle_timeout` | How long an idle connection lingers before being closed. |
| `max_lifetime` | How long any connection (idle or in-use) lives before being recycled. |
| `health_check` | Test to apply when checking out: `HealthCheck::None`, `HealthCheck::Ping`, or `HealthCheck::ResetQuery(sql)` (runs an arbitrary SQL string on every checkout via the simple-query protocol). |

A typical web service starts with `min_idle = 2`, `max_size = 16`,
`acquire_timeout = 2s`, `idle_timeout = 30s`, `max_lifetime = 30min`,
`health_check = HealthCheck::Ping`. Tune by watching p99 acquire times
and Postgres' own `pg_stat_activity` for connection churn.

## Pooled prepared statements

Each `PoolConnection` is a real, distinct Postgres connection.
Prepared statements live on the server, attached to *that* connection.
That has two consequences worth holding in your head:

- A prepared statement you make on `conn_a` is not visible from
  `conn_b`. Re-prepare on each connection (cheap — one round-trip), or
  use a shared statement cache if you build one on top.
- When the pool recycles a connection (via `max_lifetime` or a failed
  health check), all of *that connection's* prepared statements go
  with it. The next `prepare_*` call on a fresh connection rebuilds
  them.

## Errors that come from the pool itself

`pool.acquire()` returns `Result<PoolConnection, PoolError>`.
`PoolError::AcquireFailed(babar::Error)` wraps the underlying connect
error; `PoolError::Timeout` is its own variant. Translate them
into your service's error type at the boundary — the `pool` example
shows the pattern.

## Next

[Chapter 7: Bulk loads with COPY](./07-copy.md) adds the binary `COPY
FROM STDIN` path for ingesting many rows at once.
