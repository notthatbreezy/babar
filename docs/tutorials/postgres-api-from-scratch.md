# Postgres API from Scratch

This tutorial walks through a small Postgres-backed HTTP API built with:

- **Tokio** for async execution
- **Axum** for HTTP routing
- **babar** for typed Postgres access

It assumes you already know basic Rust syntax, structs, and `Result`, but have
not spent much time with Tokio yet.

We will start from an empty directory, bootstrap a tiny server, then grow it
into a coherent one-resource JSON API for tracking elephant `herds` and their grazing grounds.

## 1. Before we write code

### What we are building

By the end of this walkthrough you will have:

- a new Rust binary project
- an Axum server listening on `127.0.0.1:3000`
- a shared `babar::Pool` stored in application state
- startup code that creates a `herds` table if it does not exist yet
- a `GET /healthz` endpoint so you can prove the service is alive
- a `POST /herds` endpoint to register a herd
- a `GET /herds` endpoint to list herds
- a `GET /herds/:id` endpoint to fetch one herd

We will build that in two stages:

1. get the runtime, router, and database bootstrap in place
2. add JSON handlers on top of that working foundation

### Prerequisites

You need:

- Rust stable and `cargo`
- a running PostgreSQL server
- a shell where you can set environment variables
- basic Rust familiarity

Helpful but optional:

- `psql` so you can inspect the database manually
- the companion examples in this repository:
  - `crates/core/examples/quickstart.rs`
  - `crates/core/examples/todo_cli.rs`
  - `crates/core/examples/axum_service.rs`

### Why these tools

- **Tokio** runs async Rust code and handles network I/O.
- **Axum** gives us routing, request extraction, and JSON responses.
- **babar** gives us a typed Postgres client and pool that fit naturally into a
  Tokio application.

The main service path uses a `Pool`, not a single `Session`, because a web
server may handle many requests at once. Each request can borrow a database
connection from the pool when it needs one.

## 2. Start from an empty directory

Create a new project:

```bash
cargo init herd-api --bin
cd herd-api
```

Add the dependencies we need for the bootstrap and the API:

```bash
cargo add axum
cargo add tokio --features macros,rt-multi-thread,net
cargo add babar
cargo add serde --features derive
cargo add serde_json
cargo add tracing
cargo add tracing-subscriber --features fmt,env-filter
```

Why add `serde` now even though the first endpoint is plain text? Because the
next sections accept and return JSON, so it is simpler to install the full set
once.

### Configuration: keep it boring and explicit

For a beginner tutorial, environment variables are a good fit:

- they keep secrets like passwords out of source code
- they work the same in local dev, CI, and containers
- they avoid adding a config framework before we need one

Export these values before running the server:

```bash
export PGHOST=127.0.0.1
export PGPORT=5432
export PGUSER=postgres
export PGPASSWORD=postgres
export PGDATABASE=postgres
export API_ADDR=127.0.0.1:3000
```

If your local Postgres uses different values, change them here. `PGPASSWORD` is
the one most likely to differ.

We will also write the Rust code so local defaults exist for the whole local-dev
setup. That keeps the first run easy while still making the connection settings
obvious.

## 3. Tokio in one mental model

If you are new to Tokio, this is the shortest useful mental model:

- an `async fn` does not run by itself; it returns a value called a **future**
- a runtime polls that future and wakes it back up when it can make progress
- Tokio is the runtime that does that work for us

Why does that matter here?

- Axum waits for incoming HTTP requests
- babar waits for Postgres network reads and writes
- Tokio lets one process manage all of that waiting efficiently

When an async function hits `.await`, it is basically saying: “I cannot finish
this step right now; please come back when the socket is ready.” Tokio can then
run other work instead of blocking the whole thread.

That is why the tutorial uses:

```rust
#[tokio::main]
async fn main() { /* ... */ }
```

`#[tokio::main]` creates a Tokio runtime for the program and lets `main` be
async, so we can:

- create the Postgres pool with `.await`
- run startup SQL with `.await`
- start the Axum server with `.await`

You do not need to know every Tokio API before writing a web service. For this
tutorial, the important rule is simpler: **if something touches the network, it
will usually be async, and Tokio is what makes that async code run.**

## 4. Build the bootstrap server

Replace `src/main.rs` with this:

```rust
use std::net::SocketAddr;

use axum::routing::get;
use axum::Router;
use babar::query::Command;
use babar::{Config, Pool, PoolConfig};

#[derive(Clone)]
struct AppState {
    pool: Pool,
}

struct Settings {
    api_addr: SocketAddr,
    pg_host: String,
    pg_port: u16,
    pg_user: String,
    pg_password: String,
    pg_database: String,
}

impl Settings {
    fn from_env() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let api_addr = std::env::var("API_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:3000".into())
            .parse()?;

        let pg_host = std::env::var("PGHOST").unwrap_or_else(|_| "127.0.0.1".into());
        let pg_port = std::env::var("PGPORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(5432);
        let pg_user = std::env::var("PGUSER").unwrap_or_else(|_| "postgres".into());
        let pg_password =
            std::env::var("PGPASSWORD").unwrap_or_else(|_| "postgres".into());
        let pg_database =
            std::env::var("PGDATABASE").unwrap_or_else(|_| "postgres".into());

        Ok(Self {
            api_addr,
            pg_host,
            pg_port,
            pg_user,
            pg_password,
            pg_database,
        })
    }

    fn database_config(&self) -> Config {
        Config::new(
            &self.pg_host,
            self.pg_port,
            &self.pg_user,
            &self.pg_database,
        )
        .password(&self.pg_password)
        .application_name("herd-api")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "herd_api=info,babar=info".into()),
        )
        .with_target(false)
        .init();

    let settings = Settings::from_env()?;
    let pool = Pool::new(settings.database_config(), PoolConfig::new().max_size(8)).await?;

    initialize_schema(&pool).await?;

    let app = Router::new()
        .route("/healthz", get(healthz))
        .with_state(AppState { pool });

    tracing::info!("listening on http://{}", settings.api_addr);
    let listener = tokio::net::TcpListener::bind(settings.api_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn initialize_schema(
    pool: &Pool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = pool.acquire().await?;

    let create_herds: Command<()> = Command::raw(
        "CREATE TABLE IF NOT EXISTS herds (
            id int8 GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
            name text NOT NULL,
            grazing_ground text NOT NULL
        )",
        (),
    );

    conn.execute(&create_herds, ()).await?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}
```

### What this code is doing

There are a few important ideas packed into a small file.

#### `Settings::from_env`

This function keeps configuration loading in one place. That pays off quickly:

- `main` stays readable
- every environment variable has one obvious home
- later, if you want stricter validation, you can add it here

#### `#[tokio::main]`

This is the Tokio bridge from regular Rust into async Rust. Without it, none of
the `.await` calls in `main` would compile.

#### `Pool::new(...)`

This is the first real `babar` setup step. A pool gives the application a small
set of reusable Postgres connections. In a web service that is almost always a
better starting point than passing around one shared connection handle.

In the API section, each request handler will:

1. borrow the pool from `AppState`
2. `acquire()` a connection
3. run a typed `Command` or `Query`
4. return the connection to the pool automatically when the request finishes

#### `initialize_schema`

This tutorial keeps the schema story deliberately simple at first:

- on startup, create the one table we need
- keep the SQL visible
- avoid introducing migrations before the API itself exists

That is good enough for a beginner walkthrough and a single table. Once the app
starts growing, the next step is to move this into **babar migrations** so
schema changes are tracked explicitly instead of living inside `main.rs`.

#### `Command<()>`

Even though this SQL does not take parameters, we still use a `babar`
`Command`. The `()` means “this command expects no input values.” In the next
section we will keep using typed `Command` and `Query` values for herd inserts
and herd lookups.

## 5. Run the bootstrap

Start the server:

```bash
cargo run
```

You should see a log line like:

```text
listening on http://127.0.0.1:3000
```

In another shell, confirm the server responds:

```bash
curl http://127.0.0.1:3000/healthz
```

Expected response:

```text
ok
```

If you have `psql`, you can also confirm that startup initialization created the
table:

```bash
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" -c '\d herds'
```

If the server starts and `/healthz` returns `ok`, your bootstrap is working.

## 6. Grow the bootstrap into a herd registry API

Now replace `src/main.rs` with this fuller version:

```rust
use std::net::SocketAddr;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use babar::codec::{int8, text};
use babar::query::{Command, Query};
use babar::{Config, Pool, PoolConfig};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState {
    pool: Pool,
}

type HttpError = (StatusCode, String);

#[derive(Debug, Deserialize)]
struct CreateHerd {
    name: String,
    grazing_ground: String,
}

#[derive(Debug, Serialize)]
struct Herd {
    id: i64,
    name: String,
    grazing_ground: String,
}

struct Settings {
    api_addr: SocketAddr,
    pg_host: String,
    pg_port: u16,
    pg_user: String,
    pg_password: String,
    pg_database: String,
}

impl Settings {
    fn from_env() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let api_addr = std::env::var("API_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:3000".into())
            .parse()?;

        let pg_host = std::env::var("PGHOST").unwrap_or_else(|_| "127.0.0.1".into());
        let pg_port = std::env::var("PGPORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(5432);
        let pg_user = std::env::var("PGUSER").unwrap_or_else(|_| "postgres".into());
        let pg_password =
            std::env::var("PGPASSWORD").unwrap_or_else(|_| "postgres".into());
        let pg_database =
            std::env::var("PGDATABASE").unwrap_or_else(|_| "postgres".into());

        Ok(Self {
            api_addr,
            pg_host,
            pg_port,
            pg_user,
            pg_password,
            pg_database,
        })
    }

    fn database_config(&self) -> Config {
        Config::new(
            &self.pg_host,
            self.pg_port,
            &self.pg_user,
            &self.pg_database,
        )
        .password(&self.pg_password)
        .application_name("herd-api")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "herd_api=info,babar=info".into()),
        )
        .with_target(false)
        .init();

    let settings = Settings::from_env()?;
    let pool = Pool::new(settings.database_config(), PoolConfig::new().max_size(8)).await?;

    initialize_schema(&pool).await?;

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/herds", get(list_herds).post(create_herd))
        .route("/herds/:id", get(get_herd))
        .with_state(AppState { pool });

    tracing::info!("listening on http://{}", settings.api_addr);
    let listener = tokio::net::TcpListener::bind(settings.api_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn initialize_schema(
    pool: &Pool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = pool.acquire().await?;

    let create_herds: Command<()> = Command::raw(
        "CREATE TABLE IF NOT EXISTS herds (
            id int8 GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
            name text NOT NULL,
            grazing_ground text NOT NULL
        )",
        (),
    );

    conn.execute(&create_herds, ()).await?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn create_herd(
    State(state): State<AppState>,
    Json(payload): Json<CreateHerd>,
) -> Result<(StatusCode, Json<Herd>), HttpError> {
    let conn = state.pool.acquire().await.map_err(pool_error_http)?;

    let insert_herd: Command<(String, String)> = Command::raw(
        "INSERT INTO herds (name, grazing_ground) VALUES ($1, $2)",
        (text, text),
    );
    conn.execute(&insert_herd, (payload.name.clone(), payload.grazing_ground.clone()))
        .await
        .map_err(db_error)?;

    let current_herd_id: Query<(), (i64,)> = Query::raw(
        "SELECT currval(pg_get_serial_sequence('herds', 'id'))",
        (),
        (int8,),
    );
    let herd_id = conn
        .query(&current_herd_id, ())
        .await
        .map_err(db_error)?
        .into_iter()
        .next()
        .map(|(id,)| id)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "insert succeeded but no id was returned".to_string(),
            )
        })?;

    let select_herd: Query<(i64,), (i64, String, String)> = Query::raw(
        "SELECT id, name, grazing_ground FROM herds WHERE id = $1",
        (int8,),
        (int8, text, text),
    );
    let herd = conn
        .query(&select_herd, (herd_id,))
        .await
        .map_err(db_error)?
        .into_iter()
        .next()
        .map(herd_from_row)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "inserted herd could not be loaded back".to_string(),
            )
        })?;

    Ok((StatusCode::CREATED, Json(herd)))
}

async fn list_herds(State(state): State<AppState>) -> Result<Json<Vec<Herd>>, HttpError> {
    let conn = state.pool.acquire().await.map_err(pool_error_http)?;

    let list_herds: Query<(), (i64, String, String)> = Query::raw(
        "SELECT id, name, grazing_ground FROM herds ORDER BY id",
        (),
        (int8, text, text),
    );
    let herds = conn
        .query(&list_herds, ())
        .await
        .map_err(db_error)?
        .into_iter()
        .map(herd_from_row)
        .collect();

    Ok(Json(herds))
}

async fn get_herd(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Herd>, HttpError> {
    let conn = state.pool.acquire().await.map_err(pool_error_http)?;

    let get_herd: Query<(i64,), (i64, String, String)> = Query::raw(
        "SELECT id, name, grazing_ground FROM herds WHERE id = $1",
        (int8,),
        (int8, text, text),
    );
    let herd = conn
        .query(&get_herd, (id,))
        .await
        .map_err(db_error)?
        .into_iter()
        .next()
        .map(herd_from_row)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("herd {id} not found")))?;

    Ok(Json(herd))
}

fn herd_from_row((id, name, grazing_ground): (i64, String, String)) -> Herd {
    Herd { id, name, grazing_ground }
}

#[allow(clippy::needless_pass_by_value)]
fn pool_error_http(err: babar::PoolError) -> HttpError {
    (StatusCode::SERVICE_UNAVAILABLE, err.to_string())
}

#[allow(clippy::needless_pass_by_value)]
fn db_error(err: babar::Error) -> HttpError {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
```

This is still a small program, but now it has the three things most API
tutorials need:

- request models for incoming JSON
- response models for outgoing JSON
- handlers that turn HTTP input into typed database operations

## 7. Router, state, and handler mental model

The router is the table of contents for your service:

```rust
let app = Router::new()
    .route("/healthz", get(healthz))
    .route("/herds", get(list_herds).post(create_herd))
    .route("/herds/:id", get(get_herd))
    .with_state(AppState { pool });
```

Read it from top to bottom:

- `GET /healthz` calls `healthz`
- `GET /herds` calls `list_herds`
- `POST /herds` calls `create_herd`
- `GET /herds/:id` calls `get_herd`

`AppState` is how shared dependencies reach the handlers:

```rust
#[derive(Clone)]
struct AppState {
    pool: Pool,
}
```

Because `Pool` is stored in state, handlers do not open brand-new database
connections themselves. They borrow the shared pool, acquire one connection for
the request, and hand it back automatically when the handler returns.

That keeps the handler story simple:

1. Axum matches the route
2. Axum extracts the inputs for that route
3. the handler runs a typed database operation
4. the handler returns JSON or an HTTP error

## 8. Request and response models

The two JSON-facing structs are intentionally boring:

```rust
#[derive(Debug, Deserialize)]
struct CreateHerd {
    name: String,
    grazing_ground: String,
}

#[derive(Debug, Serialize)]
struct Herd {
    id: i64,
    name: String,
    grazing_ground: String,
}
```

`CreateHerd` is the shape we accept from clients. It does not have an `id`
because Postgres creates that for us.

`Herd` is the shape we send back. It includes the generated `id`, so clients can
fetch the herd again later.

This separation is useful even in a tiny tutorial:

- request models describe what the client must send
- response models describe what the server promises to return

## 9. How Axum extracts input

Axum handlers declare their inputs directly in the function signature.

### JSON body extraction

`create_herd` uses:

```rust
Json(payload): Json<CreateHerd>
```

That means:

- Axum reads the request body
- Axum parses it as JSON
- Axum deserializes it into `CreateHerd`

If the body is missing required fields or is not valid JSON, Axum returns an
error response before your handler logic runs.

### Path extraction

`get_herd` uses:

```rust
Path(id): Path<i64>
```

That means the `:id` portion of `/herds/:id` is parsed as an `i64`. If the
client sends `/herds/abc`, Axum rejects it because `abc` cannot become an
integer.

### State extraction

Both handlers use:

```rust
State(state): State<AppState>
```

That is how they reach the shared `Pool`.

## 10. Typed `Command` and `Query` values

The database layer is small, but it is already doing something important:
turning SQL into typed Rust values.

### Create uses a typed `Command`

The insert step is:

```rust
let insert_herd: Command<(String, String)> = Command::raw(
    "INSERT INTO herds (name, grazing_ground) VALUES ($1, $2)",
    (text, text),
);
```

Read that type literally:

- this is a `Command`
- it takes a `(String, String)` parameter tuple
- those two Rust values are encoded with the `text` codec

When the handler executes it, the payload values must match that shape:

```rust
conn.execute(&insert_herd, (payload.name.clone(), payload.grazing_ground.clone()))
    .await?;
```

That is the beginner-friendly mental model for `Command`: **write something, but
do not expect rows back**.

### Create then uses a small `Query` to load the inserted row

Because `id` is generated by the database, the handler asks Postgres for the id
that was just created on this same connection:

```rust
let current_herd_id: Query<(), (i64,)> = Query::raw(
    "SELECT currval(pg_get_serial_sequence('herds', 'id'))",
    (),
    (int8,),
);
```

Then it runs another query to fetch the full herd:

```rust
let select_herd: Query<(i64,), (i64, String, String)> = Query::raw(
    "SELECT id, name, grazing_ground FROM herds WHERE id = $1",
    (int8,),
    (int8, text, text),
);
```

This is a helpful first example of `Query`:

- the first type parameter is the input tuple
- the second type parameter is the row tuple we expect back

### List uses a typed `Query`

The list endpoint does not need parameters, so its input type is `()`:

```rust
let list_herds: Query<(), (i64, String, String)> = Query::raw(
    "SELECT id, name, grazing_ground FROM herds ORDER BY id",
    (),
    (int8, text, text),
);
```

That says: “no input values, and every row should decode as
`(i64, String, String)`.”

### Get-by-id uses a typed `Query`

The single-herd lookup takes one `i64` id and expects one decoded row shape:

```rust
let get_herd: Query<(i64,), (i64, String, String)> = Query::raw(
    "SELECT id, name, grazing_ground FROM herds WHERE id = $1",
    (int8,),
    (int8, text, text),
);
```

Notice the single-element tuple syntax:

- `(i64,)` for the Rust type
- `(int8,)` for the codec tuple

The trailing comma matters because Rust distinguishes `(i64,)` from plain `i64`.

## 11. How handlers map database results to HTTP responses

The handlers stay small because each one follows the same shape.

### Create

`create_herd`:

1. acquires a pooled connection
2. executes the typed insert command
3. queries the generated id
4. queries the inserted row
5. returns `201 Created` plus `Json<Herd>`

The return type makes that explicit:

```rust
Result<(StatusCode, Json<Herd>), HttpError>
```

### List

`list_herds` runs one query, maps each row tuple into a `Herd`, collects them
into a `Vec<Herd>`, and returns:

```rust
Result<Json<Vec<Herd>>, HttpError>
```

### Get one herd

`get_herd` runs the lookup query and then checks whether any row came back:

```rust
.into_iter()
.next()
.map(herd_from_row)
.ok_or_else(|| (StatusCode::NOT_FOUND, format!("herd {id} not found")))?;
```

That is the HTTP mapping in one place:

- row found -> `200 OK` with JSON
- no row found -> `404 Not Found`

Database failures map to `500 Internal Server Error`, and pool acquisition
failures map to `503 Service Unavailable`.

## 12. Try the finished API

Start the server:

```bash
cargo run
```

The example responses below assume a fresh `herds` table. If you already ran the
tutorial once against the same database, the returned `id` values may be higher
and `GET /herds` may include earlier rows too.

Create a herd:

```bash
curl -X POST http://127.0.0.1:3000/herds \
  -H 'content-type: application/json' \
  -d '{"name":"Royal Herd","grazing_ground":"Great Forest Meadow"}'
```

Expected response:

```json
{"id":1,"name":"Royal Herd","grazing_ground":"Great Forest Meadow"}
```

List herds:

```bash
curl http://127.0.0.1:3000/herds
```

Expected response:

```json
[{"id":1,"name":"Royal Herd","grazing_ground":"Great Forest Meadow"}]
```

Fetch one herd:

```bash
curl http://127.0.0.1:3000/herds/1
```

Expected response:

```json
{"id":1,"name":"Royal Herd","grazing_ground":"Great Forest Meadow"}
```

Ask for a herd that does not exist:

```bash
curl http://127.0.0.1:3000/herds/999
```

Expected response body:

```text
herd 999 not found
```

## 13. Add observability before production

A small async service still needs observability. Once a request can cross Axum,
Tokio, and Postgres, a plain error string stops being enough. Good logs and
traces help you answer three practical questions quickly:

- did the service start with the settings you expected?
- which request is running, and how long did it take?
- did the slow or failing step happen in HTTP handling or in Postgres?

That matters even more in async code, because `.await` lets Tokio pause one task
while other work runs. Observability gives you a breadcrumb trail back through
those pauses.

### Add request, startup, and handler tracing

We already initialized `tracing` in `main`, which is the right place to do it.
Set up the subscriber before loading settings, opening the pool, or running
startup SQL so those steps emit events too.

Add one more dependency so Axum creates a request span for every HTTP call:

```bash
cargo add tower-http --features trace
```

The changed pieces in the same `main.rs` look like this:

```rust
use std::net::SocketAddr;

use axum::extract::{MatchedPath, Path, State};
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use babar::codec::{int8, text};
use babar::query::{Command, Query};
use babar::{Config, Pool, PoolConfig};
use serde::{Deserialize, Serialize};
use tower_http::trace::TraceLayer;
use tracing::{info, instrument};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "tower_http=info,herd_api=info,babar=info".into()),
        )
        .with_target(false)
        .compact()
        .init();

    let settings = Settings::from_env()?;
    info!(
        api_addr = %settings.api_addr,
        pg_host = %settings.pg_host,
        pg_database = %settings.pg_database,
        "starting herd-api",
    );

    let pool = Pool::new(settings.database_config(), PoolConfig::new().max_size(8)).await?;
    initialize_schema(&pool).await?;
    info!("schema ready");

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/herds", get(list_herds).post(create_herd))
        .route("/herds/:id", get(get_herd))
        .with_state(AppState { pool })
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    let matched_path = request
                        .extensions()
                        .get::<MatchedPath>()
                        .map(MatchedPath::as_str)
                        .unwrap_or("<unmatched>");

                    tracing::info_span!(
                        "http.request",
                        method = %request.method(),
                        matched_path,
                    )
                })
                .on_response(|response, latency, _span| {
                    info!(
                        status = response.status().as_u16(),
                        latency_ms = latency.as_millis() as u64,
                        "request finished",
                    );
                }),
        );

    info!("listening on http://{}", settings.api_addr);
    let listener = tokio::net::TcpListener::bind(settings.api_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[instrument(name = "startup.initialize_schema", skip(pool))]
async fn initialize_schema(
    pool: &Pool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("ensuring herds table exists");
    let conn = pool.acquire().await?;

    let create_herds: Command<()> = Command::raw(
        "CREATE TABLE IF NOT EXISTS herds (
            id int8 GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
            name text NOT NULL,
            grazing_ground text NOT NULL
        )",
        (),
    );

    conn.execute(&create_herds, ()).await?;
    Ok(())
}

#[instrument(name = "handler.create_herd", skip(state, payload))]
async fn create_herd(
    State(state): State<AppState>,
    Json(payload): Json<CreateHerd>,
) -> Result<(StatusCode, Json<Herd>), HttpError> {
    info!(
        herd.name = %payload.name,
        herd.grazing_ground = %payload.grazing_ground,
        "registering herd",
    );

    let conn = state.pool.acquire().await.map_err(pool_error_http)?;
    // ... insert + select exactly as before ...
    info!(herd.id = herd_id, "herd inserted");
    Ok((StatusCode::CREATED, Json(herd)))
}

#[instrument(name = "handler.list_herds", skip(state))]
async fn list_herds(State(state): State<AppState>) -> Result<Json<Vec<Herd>>, HttpError> {
    let conn = state.pool.acquire().await.map_err(pool_error_http)?;
    // ... query exactly as before ...
    Ok(Json(herds))
}

#[instrument(name = "handler.get_herd", skip(state))]
async fn get_herd(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Herd>, HttpError> {
    let conn = state.pool.acquire().await.map_err(pool_error_http)?;
    // ... query exactly as before ...
    Ok(Json(herd))
}
```

The important idea is not “log everything.” It is “log the boundaries”:

- **startup**: selected API address, Postgres host/database, and whether schema
  initialization finished
- **incoming requests**: method, matched route, status code, and latency
- **handler-level facts**: herd ids and herd names when they help explain what
  happened
- **database work**: operation spans from babar plus safe identifiers from your
  own code

Avoid logging secrets like `PGPASSWORD`, and be careful about dumping full
request bodies once they may contain private data.

### What babar gives you for database visibility

Babar already emits `tracing` spans for its own database work, including
`db.connect`, `db.prepare`, `db.execute`, and `db.transaction`. That means the
request span from Axum can contain the lower-level database spans automatically.
If `POST /herds` slows down, you can tell whether the time went into request
routing, pool acquisition, or SQL execution instead of guessing.

### See the traces locally

Run the service with an explicit log filter:

```bash
RUST_LOG=tower_http=info,herd_api=info,babar=info cargo run
```

Then create a herd from another shell:

```bash
curl -X POST http://127.0.0.1:3000/herds \
  -H 'content-type: application/json' \
  -d '{"name":"Royal Herd","grazing_ground":"Great Forest Meadow"}'
```

You should see output shaped roughly like this:

```text
INFO starting herd-api api_addr=127.0.0.1:3000 pg_host=127.0.0.1 pg_database=postgres
INFO startup.initialize_schema: ensuring herds table exists
INFO schema ready
INFO listening on http://127.0.0.1:3000
INFO http.request{method=POST matched_path=/herds}: handler.create_herd: registering herd herd.name=Royal Herd herd.grazing_ground=Great Forest Meadow
INFO http.request{method=POST matched_path=/herds}: db.execute db.statement="INSERT INTO herds (name, grazing_ground) VALUES ($1, $2)"
INFO http.request{method=POST matched_path=/herds}: request finished status=201 latency_ms=4
```

The exact formatting depends on your subscriber, but the shape is the useful
part: one request span, nested handler activity, and database spans beneath it.

### Forward the same telemetry to Dial9 later

For local development, plain text logs to stdout are enough. In a deployed
service, keep the same span names and fields, then add an exporter or collector
layer that forwards them to your observability backend. If your team uses
Dial9, think of it as the place those traces and logs land, not as something
that changes how you instrument the herd registry itself.

A good production mental model is:

1. emit structured `tracing` events in the service
2. keep request, handler, and database spans correlated
3. attach deployment metadata like service name, environment, and version
4. ship that telemetry to Dial9 through your normal OpenTelemetry or structured
   log pipeline

That way the same instrumentation helps you both on `cargo run` and in a real
deployment.

## 14. Where to go next

At this point you have a complete beginner-sized flow:

- Axum receives HTTP input
- extractors turn that input into Rust values
- babar encodes typed parameters into SQL
- babar decodes typed rows back into Rust values
- handlers map those values into HTTP responses

When you are ready to harden it, the next practical steps are:

- move startup schema creation into **babar migrations**
- add validation rules for empty herd names or grazing grounds
- add update and delete endpoints once create/list/get feel comfortable

## Companion sources

- `crates/core/examples/quickstart.rs` — the smallest typed database flow
- `crates/core/examples/todo_cli.rs` — CRUD-shaped babar usage without HTTP
- `crates/core/examples/axum_service.rs` — the closest full HTTP + Postgres
  example in the repository
