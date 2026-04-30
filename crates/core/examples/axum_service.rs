//! Tiny Axum service backed by babar's connection pool.
//!
//! This example uses schema-scoped `query!` / `command!` wrappers for
//! application SQL. The raw fallback is reserved for the DDL setup step.
//!
//! ```text
//! cargo run -p babar --example axum_service
//! ```

use std::net::SocketAddr;

use axum::extract::{Path, Query as AxumQuery, State};
use axum::http::StatusCode;
use axum::routing::get;
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

#[derive(Debug, Default, Deserialize)]
struct ListWidgetsParams {
    name: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

type OptionalWidgetListingParams = (Option<String>, Option<i64>, Option<i64>);
type WidgetRow = (i32, String);

babar::schema! {
    mod service_schema {
        table public.widgets {
            id: primary_key(int4),
            name: text,
        },
    }
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
        .route("/widgets", get(list_widgets).post(create_widget))
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
    );
    conn.execute(&create, ()).await?;
    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

async fn list_widgets(
    State(state): State<AppState>,
    AxumQuery(params): AxumQuery<ListWidgetsParams>,
) -> Result<Json<Vec<Widget>>, (StatusCode, String)> {
    let conn = state.pool.acquire().await.map_err(pool_error_http)?;
    let select = optional_widget_listing_query();
    let rows = conn
        .query(&select, (params.name, params.limit, params.offset))
        .await
        .map_err(db_error)?;
    let widgets = rows
        .into_iter()
        .map(|(id, name)| Widget { id, name })
        .collect();
    Ok(Json(widgets))
}

fn optional_widget_listing_query() -> Query<OptionalWidgetListingParams, WidgetRow> {
    service_schema::query!(
        SELECT widgets.id, widgets.name
        FROM widgets
        WHERE (widgets.name = $name?)?
        ORDER BY widgets.id
        LIMIT $limit?
        OFFSET $offset?
    )
}

async fn create_widget(
    State(state): State<AppState>,
    Json(payload): Json<CreateWidget>,
) -> Result<(StatusCode, Json<Widget>), (StatusCode, String)> {
    let conn = state.pool.acquire().await.map_err(pool_error_http)?;
    let insert: Command<(i32, String)> =
        service_schema::command!(INSERT INTO widgets (id, name) VALUES ($id, $name));
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
    let select = service_schema::query!(
        SELECT widgets.id, widgets.name
        FROM widgets
        WHERE widgets.id = $widget_id
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
