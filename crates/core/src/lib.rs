//! `babar` — a typed, async PostgreSQL driver for Tokio that speaks the wire
//! protocol directly.
//!
//! Choose the surface by role:
//!
//! - [`query!`] / [`command!`] are the public typed-SQL entrypoints.
//! - [`schema!`] defines reusable authored schema modules with local `query!` /
//!   `command!` wrappers.
//! - [`Query::raw`](query::Query::raw) / [`Command::raw`](query::Command::raw)
//!   build zero-parameter raw SQL; [`Query::raw_with`](query::Query::raw_with) /
//!   [`Command::raw_with`](query::Command::raw_with) add explicit codecs for
//!   parameterized raw SQL.
//! - [`sql!`] builds lower-level fragments for explicit placeholder
//!   composition.
//! - A background task owns the socket so public API calls remain
//!   cancellation-safe, and errors retain SQL context, SQLSTATE metadata, and
//!   macro callsite origin.
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
//! use babar::codec::{int4, nullable, text};
//! use babar::query::{Command, Query};
//! use babar::{Config, Session};
//!
//! babar::schema! {
//!     mod app_schema {
//!         table users {
//!             id: primary_key(int4),
//!             name: text,
//!             note: nullable(text),
//!         },
//!     }
//! }
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> babar::Result<()> {
//!     let cfg = Config::new("localhost", 5432, "postgres", "postgres")
//!         .password("secret")
//!         .application_name("babar-docs");
//!     let session = Session::connect(cfg).await?;
//!
//!     let create: Command<()> = Command::raw(
//!         "CREATE TEMP TABLE users (
//!              id int4 PRIMARY KEY,
//!              name text NOT NULL,
//!              note text
//!          )",
//!     );
//!     session.execute(&create, ()).await?;
//!
//!     let insert: Command<(i32, String, Option<String>)> = app_schema::command!(
//!         INSERT INTO users (id, name, note) VALUES ($id, $name, $note)
//!     );
//!     session
//!         .execute(&insert, (1, "Ada".to_string(), Some("first".to_string())))
//!         .await?;
//!
//!     let select: Query<(), (i32, String, Option<String>)> = app_schema::query!(
//!         SELECT users.id, users.name, users.note FROM users ORDER BY users.id
//!     );
//!     let rows = session.query(&select, ()).await?;
//!     assert_eq!(rows, vec![(1, "Ada".to_string(), Some("first".to_string()))]);
//!
//!     session.close().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Typed SQL
//!
//! Public typed SQL starts with [`query!`], [`command!`], and the
//! schema-scoped wrappers that [`schema!`] emits. Inline `schema = { ... }`
//! blocks are useful for one-off examples and tests:
//! ```
//! use babar::query::Query;
//!
//! let lookup: Query<(i32,), (i32, String)> = babar::query!(
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
//! Compile-time verification is optional and online-only in v0.1:
//!
//! - `BABAR_DATABASE_URL` takes precedence over `DATABASE_URL`
//! - schema-aware `SELECT` statements from [`query!`] and schema-scoped
//!   `query!` wrappers verify referenced schema facts, parameters, and
//!   returned columns against a live PostgreSQL server when either variable is
//!   set during macro expansion
//! - non-`RETURNING` typed commands and explicit-`RETURNING` DML are not yet
//!   probe-verified through that path
//! - without configuration, the macros still compile and emit the same runtime
//!   statement values
//! - there is no offline cache or generated schema snapshot in v0.1
//!
//! ## Authored schema modules
//!
//! [`schema!`] defines Rust-visible schema modules with reusable table and
//! column symbols plus local `query!` / `command!` wrappers:
//! ```
//! use babar::query::Query;
//!
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
//! // When two SQL schemas share a table name, use the schema namespace:
//! // app_schema::public::users::id()
//! // app_schema::reporting::users::id()
//!
//! let lookup: Query<(i32,), (i32, String)> = app_schema::query!(
//!     SELECT users.id, users.name FROM users WHERE users.id = $id
//! );
//! assert_eq!(
//!     lookup.sql(),
//!     "SELECT users.id, users.name FROM users AS users WHERE (users.id = $1)"
//! );
//! ```
//!
//! The authored-schema surface is intentionally small:
//!
//! - plain `type_name` fields for ordinary columns,
//! - `nullable(type_name)` for nullable columns,
//! - `primary_key(type_name)` / `pk(type_name)` for the current semantic marker,
//! - authored Rust-visible schema modules only — no file inputs, codegen, or
//!   live introspection flow in v0.1.
//!
//! Authored declarations currently accept `bool`, `bytea`, `varchar`, `text`,
//! `int2`, `int4`, `int8`, `float4`, `float8`, `uuid`, `date`, `time`,
//! `timestamp`, `timestamptz`, `json`, `jsonb`, and `numeric`. Schema-aware
//! typed SQL lowers inferred parameters and projected rows across that same
//! family, including nullable variants; the matching babar feature must still
//! be enabled for optional families such as `uuid`, `time`, `json`, and
//! `numeric`.
//!
//! ## Raw builders
//!
//! Use raw builders when you already have SQL text and explicit codecs.
//! `raw(...)` is for zero-parameter statements; `raw_with(...)` is for raw SQL
//! that still needs a parameter encoder:
//! ```
//! use babar::query::{Command, Query};
//!
//! #[derive(Clone, Debug, PartialEq, babar::Codec)]
//! struct LookupArgs {
//!     id: i32,
//!     owner_id: i32,
//! }
//!
//! #[derive(Clone, Debug, PartialEq, babar::Codec)]
//! struct UserRow {
//!     id: i32,
//!     name: String,
//! }
//!
//! let healthcheck: Query<(), UserRow> = Query::raw(
//!     "SELECT 1::int4 AS id, 'ready'::text AS name",
//!     UserRow::CODEC,
//! );
//! assert_eq!(healthcheck.sql(), "SELECT 1::int4 AS id, 'ready'::text AS name");
//!
//! let lookup: Query<LookupArgs, UserRow> = Query::raw_with(
//!     "SELECT id, name FROM users WHERE id = $1 OR owner_id = $2",
//!     LookupArgs::CODEC,
//!     UserRow::CODEC,
//! );
//! assert_eq!(lookup.param_oids().len(), 2);
//!
//! let vacuum: Command<()> = Command::raw("VACUUM");
//! assert_eq!(vacuum.sql(), "VACUUM");
//!
//! let touch: Command<LookupArgs> = Command::raw_with(
//!     "UPDATE users SET owner_id = $2 WHERE id = $1",
//!     LookupArgs::CODEC,
//! );
//! assert_eq!(touch.param_oids().len(), 2);
//! ```
//!
//! ## `sql!` macro
//!
//! [`sql!`] is the lower-level fragment builder. It remains useful when you
//! want explicit named-placeholder composition or fragment nesting, but it is a
//! secondary surface relative to schema-aware [`query!`] / [`command!`].
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
//!
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

/// Build a schema-aware typed statement from SQL.
///
/// Start with either `schema = { ... }` for inline examples/tests or use a
/// schema-scoped wrapper such as `app_schema::query!(...)`.
pub use babar_macros::query;

/// Build a schema-aware typed command from SQL.
///
/// Non-`RETURNING` statements lower to [`query::Command`]. Statements with an
/// explicit `RETURNING` clause lower to the same query-shaped row contract as
/// other typed SQL in this release. Use inline `schema = { ... }` blocks for
/// small examples/tests or schema-scoped wrappers such as `app_schema::command!(...)`.
pub use babar_macros::command;

/// Declare an authored schema module with reusable table and column symbols.
///
/// The generated module exposes `SCHEMA`, table/column markers, and local
/// `query!` / `command!` wrappers that reuse the authored schema facts.
pub use babar_macros::schema;

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
pub use babar_macros::sql;

pub use babar_macros::Codec;

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
