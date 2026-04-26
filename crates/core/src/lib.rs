//! `babar` — a typed, async Postgres driver for Tokio that speaks the
//! `PostgreSQL` wire protocol directly.
//!
//! The current public surface includes typed commands and queries, reusable
//! prepared statements, and portal-backed row streaming on top of a
//! cancellation-safe background driver task.
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

pub(crate) mod auth;
pub mod codec;
mod config;
mod error;
pub(crate) mod protocol;
pub mod query;
pub mod types;

// `tokio::net` is unavailable under `--cfg loom`; the session machinery
// uses TcpStream and so must be cfg-gated. Pure modules above remain
// available so loom tests can import e.g. `babar::Error` if they want.
#[cfg(not(loom))]
mod session;

pub use config::Config;
pub use error::{Error, Result};
#[cfg(not(loom))]
pub use session::{PreparedCommand, PreparedQuery, RawRows, RowStream, ServerParams, Session};
