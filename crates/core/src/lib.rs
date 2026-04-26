//! `babar` — a typed, async PostgreSQL driver for Tokio that speaks the wire
//! protocol directly.
//!
//! `babar` deliberately keeps the API explicit:
//!
//! - SQL is a typed value (`Query`, `Command`, `PreparedQuery`, ...).
//! - Codecs are imported values (`int4`, `text`, `uuid`, ...), not inferred.
//! - A background task owns the socket so public API calls remain
//!   cancellation-safe.
//! - Errors retain SQL context, SQLSTATE metadata, and `sql!` callsite origin.
//!
//! ## Feature highlights
//!
//! - simple-query, extended query, prepared-statement, transaction, and pool APIs
//! - text codecs in core plus opt-in `uuid`, `time`, `chrono`, `json`,
//!   `numeric`, `net`, `interval`, `array`, and `range` modules
//! - optional TLS via the `rustls` feature (default) or `native-tls`
//! - OpenTelemetry-friendly `tracing` spans for connect / prepare / execute /
//!   transaction flows
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

pub(crate) mod auth;
pub mod codec;
mod config;
mod error;
#[cfg(not(loom))]
pub mod pool;
pub(crate) mod protocol;
pub mod query;
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
/// The macro only rewrites placeholders and captures source origin metadata. It
/// does not connect to Postgres, infer codecs, quote identifiers, or validate
/// output columns.
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
pub use babar_macros::{sql, Codec};

#[doc(hidden)]
pub mod __private {
    pub use bytes::Bytes;
}

#[cfg(not(loom))]
mod session;

pub use config::{Config, TlsBackend, TlsMode};
pub use error::{Error, Result};
#[cfg(not(loom))]
pub use pool::{
    HealthCheck, Pool, PoolConfig, PoolConnection, PoolError, PooledPreparedCommand,
    PooledPreparedQuery, PooledRowStream, PooledSavepoint, PooledTransaction,
};
#[cfg(not(loom))]
pub use session::{PreparedCommand, PreparedQuery, RawRows, RowStream, ServerParams, Session};
#[cfg(not(loom))]
pub use transaction::{Savepoint, Transaction};
