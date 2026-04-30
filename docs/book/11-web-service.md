# 11. Building a web service

In this chapter you'll wire babar into an Axum HTTP service: a connection pool
in shared state, JSON in / JSON out handlers, and schema-aware typed SQL for the
application queries.

## Setup

```rust
use std::net::SocketAddr;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use babar::query::{Command, Query};
use babar::{Config, Pool, PoolConfig};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState {
    pool: Pool,
}

#[derive(Debug, Serialize)]
struct Widget {
    id: i32,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CreateWidget {
    id: i32,
    name: String,
}

babar::schema! {
    mod app_schema {
        table public.widgets {
            id: primary_key(int4),
            name: text,
        }
    }
}

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
async fn initialize(pool: &Pool) -> babar::Result<()> {
    let conn = pool.acquire().await.map_err(pool_error)?;
    let create: Command<()> = Command::raw(
        "CREATE TABLE IF NOT EXISTS widgets (id int4 PRIMARY KEY, name text NOT NULL)",
        (),
    );
    conn.execute(&create, ()).await?;
    Ok(())
}

async fn create_widget(
    State(state): State<AppState>,
    Json(payload): Json<CreateWidget>,
) -> Result<(StatusCode, Json<Widget>), (StatusCode, String)> {
    let conn = state.pool.acquire().await.map_err(pool_http)?;
    let insert: Command<(i32, String)> =
        app_schema::command!(INSERT INTO widgets (id, name) VALUES ($id, $name));
    conn.execute(&insert, (payload.id, payload.name.clone()))
        .await
        .map_err(db_http)?;
    Ok((StatusCode::CREATED, Json(Widget { id: payload.id, name: payload.name })))
}

async fn get_widget(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<Widget>, (StatusCode, String)> {
    let conn = state.pool.acquire().await.map_err(pool_http)?;
    let select: Query<(i32,), (i32, String)> = app_schema::query!(
        SELECT widgets.id, widgets.name
        FROM widgets
        WHERE widgets.id = $widget_id
    );
    let rows = conn.query(&select, (id,)).await.map_err(db_http)?;
    rows.into_iter()
        .next()
        .map(|(id, name)| Json(Widget { id, name }))
        .ok_or((StatusCode::NOT_FOUND, format!("widget {id} not found")))
}
```

Each handler:

1. pulls a connection from the pool with `pool.acquire()`
2. uses schema-aware `query!` / `command!` for application SQL
3. reserves `Command::raw` for unsupported setup SQL such as the DDL in `initialize`
4. maps `babar::Error` and `babar::PoolError` to HTTP responses at the boundary

Drop the connection between handlers — Axum will get a fresh one for the next
request. Pass the `Pool`, not a long-lived connection handle, through your own
service types.

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

Use the SQLSTATE table from [Chapter 9](./09-error-handling.md) to expand this
map. Resist the temptation to expose `Error`'s full `Display` directly — it's
great for logs, but it leaks internals to clients.

## Raw vs simple-query in service code

For most handlers, stick to `query!` / `command!`. When you need a fallback:

- use `Query::raw` / `Command::raw` for unsupported single statements that
  should still use the extended protocol and typed params/rows
- use `simple_query_raw` only for simple-protocol raw SQL strings, especially
  multi-statement bootstrap or maintenance work

That same split is why the example uses `Command::raw` for table setup instead
of `simple_query_raw`: it is still one statement, still fits the extended
protocol, and does not need raw result sets.

## Where the spans come from

Once `tracing_subscriber` is initialized (any subscriber will do — `fmt`,
`tracing-opentelemetry`, etc.), every `Session::connect`, `Session::execute`,
`Session::query`, prepared statement, and transaction call records a span:

| Span name | Fields |
|---|---|
| `db.connect` | `db.system`, `db.user`, `db.name`, `net.peer.name`, `net.peer.port` |
| `db.prepare` | `db.system`, `db.statement`, `db.operation` |
| `db.execute` | `db.system`, `db.statement`, `db.operation` |
| `db.transaction` | `db.system`, `db.operation` |

Field names follow OpenTelemetry semantic conventions, so any exporter that
understands OTel naming gets useful signal for free.

## What this gets you

The full `axum_service` example in `crates/core/examples/axum_service.rs` adds
env-var parsing plus a listing endpoint, but it keeps the same shape: pool in
state, one acquire per handler, schema-aware typed SQL for application queries,
and raw fallbacks only where the typed subset is not the right tool.

## Next

[Chapter 12: TLS & security](./12-tls.md) covers `TlsMode`, root certificates,
and the SCRAM-SHA-256 channel-binding handshake.
