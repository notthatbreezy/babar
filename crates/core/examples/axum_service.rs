//! Tiny Axum service backed by babar's connection pool.
//!
//! ```text
//! cargo run -p babar --example axum_service
//! ```

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "babar=info,axum_service=info".into()),
        )
        .with_target(false)
        .try_init()
        .ok();

    let cfg = Config::new(
        std::env::var("PGHOST").unwrap_or_else(|_| "127.0.0.1".into()),
        std::env::var("PGPORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(5432),
        std::env::var("PGUSER").unwrap_or_else(|_| "postgres".into()),
        std::env::var("PGDATABASE").unwrap_or_else(|_| "postgres".into()),
    )
    .password(std::env::var("PGPASSWORD").unwrap_or_else(|_| "postgres".into()))
    .application_name("babar-axum-service");

    let pool = Pool::new(cfg, PoolConfig::new().max_size(8)).await?;
    initialize(&pool).await?;

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/widgets", post(create_widget))
        .route("/widgets/:id", get(get_widget))
        .with_state(AppState { pool });

    let addr: SocketAddr = std::env::var("AXUM_SERVICE_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".into())
        .parse()?;
    println!("listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

async fn initialize(pool: &Pool) -> babar::Result<()> {
    let conn = pool.acquire().await.map_err(pool_error)?;
    let create: Command<()> = Command::raw(
        "CREATE TABLE IF NOT EXISTS widgets (id int4 PRIMARY KEY, name text NOT NULL)",
        (),
    );
    conn.execute(&create, ()).await?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn create_widget(
    State(state): State<AppState>,
    Json(payload): Json<CreateWidget>,
) -> Result<(StatusCode, Json<Widget>), (StatusCode, String)> {
    let conn = state.pool.acquire().await.map_err(pool_error_http)?;
    let insert: Command<(i32, String)> = Command::raw(
        "INSERT INTO widgets (id, name) VALUES ($1, $2)",
        (int4, text),
    );
    conn.execute(&insert, (payload.id, payload.name.clone()))
        .await
        .map_err(db_error)?;
    Ok((
        StatusCode::CREATED,
        Json(Widget {
            id: payload.id,
            name: payload.name,
        }),
    ))
}

async fn get_widget(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<Widget>, (StatusCode, String)> {
    let conn = state.pool.acquire().await.map_err(pool_error_http)?;
    let select: Query<(i32,), (i32, String)> = Query::raw(
        "SELECT id, name FROM widgets WHERE id = $1",
        (int4,),
        (int4, text),
    );
    let rows = conn.query(&select, (id,)).await.map_err(db_error)?;
    let Some((id, name)) = rows.into_iter().next() else {
        return Err((StatusCode::NOT_FOUND, format!("widget {id} not found")));
    };
    Ok(Json(Widget { id, name }))
}

fn pool_error(err: babar::PoolError) -> babar::Error {
    match err {
        babar::PoolError::AcquireFailed(err) => err,
        other => babar::Error::Config(other.to_string()),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn pool_error_http(err: babar::PoolError) -> (StatusCode, String) {
    (StatusCode::SERVICE_UNAVAILABLE, err.to_string())
}

#[allow(clippy::needless_pass_by_value)]
fn db_error(err: babar::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
