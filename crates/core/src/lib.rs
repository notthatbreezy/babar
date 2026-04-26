//! `babar` — a typed, async Postgres driver for Tokio that speaks the
//! `PostgreSQL` wire protocol directly.
//!
//! The current public surface includes typed commands and queries, reusable
//! prepared statements, and portal-backed row streaming on top of a
//! cancellation-safe background driver task.
//!
//! ## `sql!` macro
//!
//! [`sql!`] is the preferred way to build SQL fragments in babar. It takes a
//! SQL string that uses named placeholders like `$id` or `$name`, plus an
//! explicit codec for each placeholder, and returns a [`query::Fragment`] with
//! a flat Rust tuple parameter type.
//!
//! The macro renumbers placeholders to Postgres' native `$1`, `$2`, ...
//! protocol form, reuses the same slot when the same named placeholder appears
//! multiple times, and records the callsite in [`query::Origin`] for error
//! reporting.
//!
//! ```
//! use babar::codec::{int4, text};
//! use babar::query::Query;
//! use babar::sql;
//!
//! let q: Query<(i32, String), (i32, String)> = Query::from_fragment(
//!     sql!(
//!         "SELECT id, name FROM users WHERE id = $id OR owner = $name OR name = $name",
//!         id = int4,
//!         name = text,
//!     ),
//!     (int4, text),
//! );
//!
//! assert_eq!(
//!     q.sql(),
//!     "SELECT id, name FROM users WHERE id = $1 OR owner = $2 OR name = $2"
//! );
//! ```
//!
//! `sql!` does **not** talk to a database at compile time, infer codecs, or
//! validate result-column schemas. It only rewrites placeholders, checks that
//! every `$name` has exactly one binding, and builds the fragment. Server-side
//! validation still happens when you prepare or execute the statement. The
//! macro also does not interpolate identifiers or arbitrary SQL text: values
//! must stay parameterized, and dynamic SQL structure should be assembled from
//! trusted fragments in Rust.
//!
//! ## Architecture
//!
//! Every connection is owned by a background driver task. The user holds a
//! [`Session`] which is a thin handle that sends commands over an `mpsc`
//! channel and receives responses on per-command `oneshot` channels. The
//! driver is the sole writer to the socket; user-facing API calls are
//! cancellation-safe by construction.
//!
//! ```text
//!     User code → Session (mpsc handle)
//!                       ↓
//!             Background driver task (owns TcpStream, state machine)
//!                       ↓
//!             Postgres server (wire protocol)
//! ```
//!
//! ## Prepared statements
//!
//! The prepared-statement lifecycle is:
//!
//! 1. Build a [`query::Query`] or [`query::Command`] with explicit codecs.
//! 2. Call [`Session::prepare_query`] or [`Session::prepare_command`] once.
//!    The driver sends `Parse`/`Describe`, validates the returned schema, and
//!    caches the statement per session by SQL text plus parameter OIDs.
//! 3. Reuse the returned [`PreparedQuery`] / [`PreparedCommand`] as many times
//!    as needed.
//! 4. Call `.close().await` for confirmed cleanup, or let the last handle drop
//!    for best-effort `Close`/deallocate on the server.
//!
//! ```no_run
//! use babar::codec::int4;
//! use babar::query::Query;
//! use babar::{Config, Session};
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> babar::Result<()> {
//!     let cfg = Config::new("localhost", 5432, "postgres", "postgres")
//!         .password("secret")
//!         .application_name("babar-docs-prepared");
//!     let session = Session::connect(cfg).await?;
//!
//!     let q: Query<(i32,), (i32,)> =
//!         Query::raw("SELECT $1::int4 + 1", (int4,), (int4,));
//!     let prepared = session.prepare_query(&q).await?;
//!
//!     assert_eq!(prepared.query((41,)).await?, vec![(42,)]);
//!     assert_eq!(prepared.query((99,)).await?, vec![(100,)]);
//!
//!     prepared.close().await?;
//!     session.close().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Streaming
//!
//! [`Session::stream`] and [`Session::stream_with_batch_size`] run a query
//! through a server-side portal and fetch rows in bounded `Execute(max_rows)`
//! batches. This keeps memory bounded and naturally applies backpressure when
//! the consumer is slower than the database.
//!
//! Draining the stream closes the portal automatically. Dropping it early stops
//! future fetches and best-effort closes the temporary portal and statement.
//!
//! ```no_run
//! use babar::codec::int4;
//! use babar::query::Query;
//! use babar::{Config, Session};
//! use futures_util::StreamExt;
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> babar::Result<()> {
//!     let cfg = Config::new("localhost", 5432, "postgres", "postgres")
//!         .password("secret")
//!         .application_name("babar-docs-stream");
//!     let session = Session::connect(cfg).await?;
//!
//!     let q: Query<(), (i32,)> = Query::raw(
//!         "SELECT gs::int4 FROM generate_series(1, 5) AS gs ORDER BY gs",
//!         (),
//!         (int4,),
//!     );
//!     let mut rows = session.stream_with_batch_size(&q, (), 2).await?;
//!     while let Some(row) = rows.next().await {
//!         println!("{:?}", row?);
//!     }
//!
//!     session.close().await?;
//!     Ok(())
//! }
//! ```
//!
//! See `cargo run -p babar --example prepared_and_stream` for a complete M2
//! example that combines both patterns.
//!
//! ## Stability
//!
//! Nothing in this crate is stable yet. The public surface is still evolving
//! and may change in every milestone up to v0.1.

#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate self as babar;

pub(crate) mod auth;
pub mod codec;
mod config;
mod error;
pub(crate) mod protocol;
pub mod query;
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
/// Build typed SQL fragments with named `$name` placeholders.
///
/// ```
/// use babar::codec::{bool, int4, text};
/// use babar::query::{Command, Query};
///
/// let query: Query<(i32, bool), (String,)> = Query::from_fragment(
///     babar::sql!(
///         "SELECT name FROM users WHERE ($filter) AND active = $active",
///         filter = babar::sql!("id = $id OR owner_id = $id", id = int4),
///         active = bool,
///     ),
///     (text,),
/// );
/// assert_eq!(
///     query.sql(),
///     "SELECT name FROM users WHERE (id = $1 OR owner_id = $1) AND active = $2"
/// );
///
/// let command: Command<(i32, String)> = Command::from_fragment(babar::sql!(
///     "INSERT INTO users (id, name) VALUES ($id, $name)",
///     id = int4,
///     name = text,
/// ));
/// assert_eq!(
///     command.sql(),
///     "INSERT INTO users (id, name) VALUES ($1, $2)"
/// );
/// ```
pub use babar_macros::sql;

// `tokio::net` is unavailable under `--cfg loom`; the session machinery
// uses TcpStream and so must be cfg-gated. Pure modules above remain
// available so loom tests can import e.g. `babar::Error` if they want.
#[cfg(not(loom))]
mod session;

pub use config::Config;
pub use error::{Error, Result};
#[cfg(not(loom))]
pub use session::{PreparedCommand, PreparedQuery, RawRows, RowStream, ServerParams, Session};
