# 11. Building a web service

In this chapter you'll wire babar into an Axum HTTP service: a
connection pool in your shared state, JSON in / JSON out handlers,
and clean error mapping at the boundary.

## Setup

```rust
use std::net::SocketAddr;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use babar::codec::{int4, text};
use babar::query::{Command, Query};
use babar::{Config, Pool, PoolConfig};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState {
    pool: Pool,                                                       // type: Pool
}

#[derive(Debug, Serialize)]
struct Widget { id: i32, name: String }

#[derive(Debug, Deserialize)]
struct CreateWidget { id: i32, name: String }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "babar=info".into()))
        .try_init()
        .ok();

    let cfg = Config::new("127.0.0.1", 5432, "postgres", "postgres")
        .password("postgres")
        .application_name("babar-axum-service");
    let pool = Pool::new(cfg, PoolConfig::new().max_size(8)).await?;

    initialize(&pool).await?;

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/widgets", post(create_widget))
        .route("/widgets/:id", get(get_widget))
        .with_state(AppState { pool });

    let addr: SocketAddr = "127.0.0.1:3000".parse()?;
    println!("listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
```

## The handler shape

```rust
async fn create_widget(
    State(state): State<AppState>,
    Json(payload): Json<CreateWidget>,
) -> Result<(StatusCode, Json<Widget>), (StatusCode, String)> {
    let conn = state.pool.acquire().await.map_err(pool_http)?;
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO widgets (id, name) VALUES ($1, $2)",
        (int4, text),
    );
    conn.execute(&insert, (payload.id, payload.name.clone())).await.map_err(db_http)?;
    Ok((StatusCode::CREATED, Json(Widget { id: payload.id, name: payload.name })))
}

async fn get_widget(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<Widget>, (StatusCode, String)> {
    let conn = state.pool.acquire().await.map_err(pool_http)?;
    let select: Query<(i32,), (i32, String)> = Query::raw(
        "SELECT id, name FROM widgets WHERE id = $1",
        (int4,),
        (int4, text),
    );
    let rows = conn.query(&select, (id,)).await.map_err(db_http)?;
    rows.into_iter().next()
        .map(|(id, name)| Json(Widget { id, name }))
        .ok_or((StatusCode::NOT_FOUND, format!("widget {id} not found")))
}
```

Each handler:

1. Pulls a connection from the pool with `pool.acquire()`. The
   handle is dropped at the end of the function and returns to the
   pool automatically.
2. Builds a typed `Command` or `Query` and runs it.
3. Maps `babar::Error` and `babar::PoolError` to `(StatusCode,
   String)` at the boundary.

Drop the connection between handlers — Axum will get a fresh one for
the next request. Don't pass a `PoolConnection` through your service's
own types; pass the `Pool` and acquire when you need to. That's how
you keep request handlers cheap to spin up.

## Errors at the boundary

```rust
fn pool_http(err: babar::PoolError) -> (StatusCode, String) {
    (StatusCode::SERVICE_UNAVAILABLE, err.to_string())
}

fn db_http(err: babar::Error) -> (StatusCode, String) {
    match err {
        babar::Error::Server { code, .. } if code == "23505" => {
            (StatusCode::CONFLICT, "already exists".into())
        }
        babar::Error::Server { code, .. } if code == "23503" => {
            (StatusCode::UNPROCESSABLE_ENTITY, "foreign key violation".into())
        }
        other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    }
}
```

Use the SQLSTATE table from
[Chapter 9](./09-error-handling.md) to expand this map. Resist the
temptation to expose `Error`'s `Display` directly — it's great for
logs, but it leaks internals to clients.

## Where the spans come from

Once `tracing_subscriber` is initialized (any subscriber will do —
`fmt`, `tracing-opentelemetry`, etc.), every `Session::connect`,
`Session::execute`, `Session::query`, prepared statement, and
transaction call records a span:

| Span name | Fields |
|---|---|
| `db.connect` | `db.system`, `db.user`, `db.name`, `net.peer.name`, `net.peer.port` |
| `db.prepare` | `db.system`, `db.statement`, `db.operation` |
| `db.execute` | `db.system`, `db.statement`, `db.operation` |
| `db.transaction` | `db.system`, `db.operation` |

Field names follow OpenTelemetry semantic conventions, so any
exporter that understands OTel naming gets useful signal for free.
There's no babar-specific subscriber to register; configure the
subscriber you'd configure anyway.

## What this gets you

The full `axum_service` example in
`crates/core/examples/axum_service.rs` is a few dozen lines longer
(env var parsing, two more routes), but it's the same shape. Once you
have a `Pool` plus a couple of helper functions for error mapping,
adding a new endpoint is just another typed `Query` and another
`acquire()`.

## Next

[Chapter 12: TLS & security](./12-tls.md) covers `TlsMode`, root
certificates, and the SCRAM-SHA-256 channel-binding handshake.
