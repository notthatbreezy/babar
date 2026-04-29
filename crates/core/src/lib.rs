//! `babar` — a typed, async PostgreSQL driver for Tokio that speaks the wire
//! protocol directly.
//!
//! `babar` deliberately keeps the API explicit:
//!
//! - SQL is a typed value (`Query`, `Command`, `PreparedQuery`, ...).
//! - Query codecs are imported values (`int4`, `text`, `uuid`, ...); `#[derive(Codec)]`
//!   infers common struct fields and accepts `#[pg(codec = "...")]` overrides.
//! - A background task owns the socket so public API calls remain
//!   cancellation-safe.
//! - Errors retain SQL context, SQLSTATE metadata, and `sql!` callsite origin.
//!
//! ## Feature highlights
//!
//! - simple-query, extended query, prepared-statement, transaction, pool, and
//!   binary `COPY FROM STDIN` bulk-ingest APIs
//! - text codecs in core plus opt-in `uuid`, `time`, `chrono`, `json`,
//!   `numeric`, `net`, `interval`, `array`, `range`, `multirange`, PostGIS
//!   spatial codecs via `postgis`, plus `pgvector`, `text-search`, `macaddr`,
//!   `bits`, `hstore`, and `citext` modules
//! - optional TLS via the `rustls` feature (default) or `native-tls`
//! - OpenTelemetry-friendly `tracing` spans for connect / prepare / execute /
//!   transaction flows
//!
//! COPY support is intentionally narrow in v0.1: babar ships typed binary
//! `COPY FROM STDIN` bulk ingest via [`CopyIn`], while `COPY TO`, text/CSV
//! modes, and broader replication-style COPY flows remain out of scope.
//!
//! Important codec caveats in v0.1:
//!
//! - `postgis`, `pgvector`, `hstore`, and `citext` require the matching
//!   PostgreSQL extension to exist in the target database.
//! - PostGIS support is limited to common 2D EWKB shapes; Z/M geometries,
//!   `GeometryCollection`, and PostgreSQL built-in geometric types are not
//!   supported.
//! - `TsVector` / `TsQuery` keep canonical SQL text rather than exposing a
//!   structured Rust AST.
//! - `range` / `multirange` currently cover PostgreSQL's built-in scalar range
//!   families only (`int4`, `int8`, `numeric`, `date`, `timestamp`,
//!   `timestamptz`).
//!
//! ## Quick start
//!
//! ```no_run
//! use babar::codec::{int4, text};
//! use babar::query::{Command, Query};
//! use babar::{Config, Session};
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> babar::Result<()> {
//!     let cfg = Config::new("localhost", 5432, "postgres", "postgres")
//!         .password("secret")
//!         .application_name("babar-docs");
//!     let session = Session::connect(cfg).await?;
//!
//!     let create: Command<()> = Command::raw(
//!         "CREATE TEMP TABLE users (id int4 PRIMARY KEY, name text NOT NULL)",
//!         (),
//!     );
//!     session.execute(&create, ()).await?;
//!
//!     let insert: Command<(i32, String)> = Command::raw(
//!         "INSERT INTO users (id, name) VALUES ($1, $2)",
//!         (int4, text),
//!     );
//!     session.execute(&insert, (1, "Ada".to_string())).await?;
//!
//!     let select: Query<(), (i32, String)> = Query::raw(
//!         "SELECT id, name FROM users ORDER BY id",
//!         (),
//!         (int4, text),
//!     );
//!     let rows = session.query(&select, ()).await?;
//!     assert_eq!(rows, vec![(1, "Ada".to_string())]);
//!
//!     session.close().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## `sql!` macro
//!
//! [`sql!`] builds a [`query::Fragment`] from SQL that uses named placeholders
//! like `$id` or `$name`, captures the macro callsite in [`query::Origin`], and
//! rewrites placeholders into PostgreSQL's native `$1`, `$2`, ... numbering.
//!
//! ```
//! use babar::codec::{int4, text};
//! use babar::query::Query;
//! use babar::sql;
//!
//! let q: Query<(i32, String), (i32, String)> = sql!(
//!     "SELECT id, name FROM users WHERE id = $id OR owner = $name OR name = $name",
//!     id = int4,
//!     name = text,
//! )
//! .query((int4, text));
//!
//! assert_eq!(
//!     q.sql(),
//!     "SELECT id, name FROM users WHERE id = $1 OR owner = $2 OR name = $2"
//! );
//! ```
//!
//! ## `query!`, `command!`, and `typed_query!` macros
//!
//! [`query!`] and [`command!`] build ordinary [`query::Query`] /
//! [`query::Command`] values directly from positional SQL plus a narrow,
//! explicit codec DSL:
//!
//! - scalars: `int2`, `int4`, `int8`, `bool`, `text`, `varchar`, `bytea`
//! - nullable scalars: `nullable(...)`
//! - tuples of the above, including `()` for zero parameters
//!
//! Compile-time verification is optional and online-only in v0.1:
//!
//! - `BABAR_DATABASE_URL` takes precedence over `DATABASE_URL`
//! - [`query!`] / [`command!`] verify declared parameter and row shapes against a
//!   live PostgreSQL server when either variable is set during macro expansion
//! - [`sql!`] reuses the same probe for parameter metadata only, and only when
//!   every binding codec is in the verifiable subset
//! - without configuration, the macros still compile and emit the same runtime
//!   statement values
//! - there is no offline cache or generated schema snapshot in v0.1
//!
//! ```
//! use babar::codec::{int4, text};
//!
//! let lookup = babar::query!(
//!     "SELECT id, name FROM users WHERE id = $1",
//!     params = (int4,),
//!     row = (int4, text),
//! );
//! assert_eq!(lookup.sql(), "SELECT id, name FROM users WHERE id = $1");
//!
//! let insert = babar::command!(
//!     "INSERT INTO users (id, name) VALUES ($1, $2)",
//!     params = (int4, text),
//! );
//! assert_eq!(insert.sql(), "INSERT INTO users (id, name) VALUES ($1, $2)");
//! ```
//!
//! [`typed_query!`] is the greenfield typed-SQL entrypoint. It accepts a small
//! inline schema DSL that the proc macro can read directly at expansion time,
//! avoiding any need to evaluate user-defined Rust `const` schema symbols:
//!
//! ```
//! use babar::query::Query;
//!
//! let lookup: Query<(i32,), (i32, String)> = babar::typed_query!(
//!     schema = {
//!         table public.users {
//!             id: int4,
//!             name: text,
//!             active: bool,
//!         },
//!     },
//!     SELECT users.id, users.name FROM users WHERE users.id = $id AND users.active = true
//! );
//!
//! assert_eq!(
//!     lookup.sql(),
//!     "SELECT users.id, users.name FROM users AS users WHERE ((users.id = $1) AND (users.active = TRUE))"
//! );
//! ```
//!
//! For reusable authored declarations, [`schema!`] can define Rust-visible schema
//! modules with multiple tables and narrow field markers:
//!
//! ```
//! babar::schema! {
//!     pub mod app_schema {
//!         table public.users {
//!             id: primary_key(int4),
//!             name: text,
//!             deleted_at: nullable(timestamptz),
//!         },
//!         table public.posts {
//!             id: pk(int8),
//!             author_id: int4,
//!         },
//!     }
//! }
//!
//! assert!(app_schema::users::id().is_primary_key());
//! assert_eq!(app_schema::posts::author_id().sql_type(), babar::schema::SqlType::INT4);
//! assert_eq!(app_schema::SCHEMA.tables().len(), 2);
//! ```
//!
//! ## TLS
//!
//! Enable the default `rustls` feature (or the optional `native-tls` feature),
//! then request TLS explicitly via [`Config::require_tls`]. When the server
//! offers SCRAM-SHA-256-PLUS, babar binds SCRAM to the TLS certificate
//! automatically. When connecting by IP address, set [`Config::tls_server_name`]
//! so SNI and certificate verification use the intended DNS name.
//!
//! ```no_run
//! use babar::{Config, TlsMode};
//!
//! let _ = Config::new("db.internal", 5432, "app", "app")
//!     .password("secret")
//!     .tls_mode(TlsMode::Require);
//! ```
//!
//! ## Architecture
//!
//! ```text
//! User code → Session / Pool handle
//!                    ↓
//!        background driver task (owns transport and protocol state)
//!                    ↓
//!              PostgreSQL server
//! ```
//!
//! The driver task is the sole owner of the transport. Public futures communicate
//! with it over `mpsc` + `oneshot`, which keeps cancellation localized to the
//! caller while the protocol state machine continues to completion.

#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate self as babar;

#[doc(hidden)]
pub mod async_fn;
pub(crate) mod auth;
pub mod codec;
mod config;
pub mod copy;
mod error;
pub mod migration;
#[cfg(not(loom))]
pub mod pool;
pub(crate) mod protocol;
pub mod query;
pub mod schema;
pub(crate) mod telemetry;
#[cfg(not(loom))]
pub mod tls;
#[cfg(not(loom))]
pub mod transaction;
pub mod types;

/// Build a [`query::Fragment`] from SQL that uses named placeholders.
///
/// Placeholders use the v0.1 syntax `$name`. Each placeholder must have a
/// matching `name = codec` binding, and repeating the same placeholder reuses
/// the same parameter slot. Nested `sql!(...)` calls are allowed and flatten
/// into one fragment with left-to-right parameter ordering.
///
/// The macro rewrites placeholders, captures source origin metadata, and — when
/// compile-time verification is configured through `BABAR_DATABASE_URL` or
/// `DATABASE_URL` and every binding codec is in the verifiable subset —
/// validates parameter metadata against a live PostgreSQL server. Unsupported
/// binding codecs simply skip verification. The macro does not infer codecs,
/// quote identifiers, or validate output columns, and v0.1 does not ship an
/// offline verification cache.
///
/// ```
/// use babar::codec::{bool, int4, text};
/// use babar::query::{Command, Query};
///
/// let query: Query<(i32, bool), (String,)> = babar::sql!(
///     "SELECT name FROM users WHERE ($filter) AND active = $active",
///     filter = babar::sql!("id = $id OR owner_id = $id", id = int4),
///     active = bool,
/// )
/// .query((text,));
/// assert_eq!(
///     query.sql(),
///     "SELECT name FROM users WHERE (id = $1 OR owner_id = $1) AND active = $2"
/// );
///
/// let command: Command<(i32, String)> = babar::sql!(
///     "INSERT INTO users (id, name) VALUES ($id, $name)",
///     id = int4,
///     name = text,
/// )
/// .command();
/// assert_eq!(
///     command.sql(),
///     "INSERT INTO users (id, name) VALUES ($1, $2)"
/// );
/// ```
pub use babar_macros::{command, query, schema, sql, typed_query, Codec};

#[doc(hidden)]
pub mod __private {
    pub use crate::query::{push_bound_param, push_null_param, toggle, BoundStatement, Toggle};
    pub use bytes::Bytes;
}

#[cfg(not(loom))]
mod session;

pub use config::{Config, TlsBackend, TlsMode};
pub use copy::CopyIn;
pub use error::{Error, Result};
pub use migration::{MigrationError, Migrator, MigratorOptions};
#[cfg(not(loom))]
pub use pool::{
    HealthCheck, Pool, PoolConfig, PoolConnection, PoolError, PooledPreparedCommand,
    PooledPreparedQuery, PooledRowStream, PooledSavepoint, PooledTransaction,
};
#[cfg(not(loom))]
pub use session::{PreparedCommand, PreparedQuery, RawRows, RowStream, ServerParams, Session};
#[cfg(not(loom))]
pub use transaction::{Savepoint, Transaction};
