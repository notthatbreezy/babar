//! [`Session`] — the public, cancellation-safe handle to a Postgres
//! connection.
//!
//! The `Session` itself is just a wrapper around a [`tokio::sync::mpsc`]
//! sender. All real work happens on the background driver task spawned by
//! [`Session::connect`]. This split is what makes the API
//! cancellation-safe: dropping the future returned by [`Session::simple_query_raw`]
//! cannot leave the connection in an inconsistent state because the driver
//! continues to drive the protocol to completion regardless of what the
//! caller does.

mod driver;
mod startup;

use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::query::{Command as QueryCommand, Query};

pub(crate) use driver::Command;
pub use driver::{RawRows, ServerParams};

/// Channel buffer between user code and the driver. Backpressure here just
/// rate-limits how fast users can enqueue commands; the driver handles
/// them in order.
const COMMAND_BUFFER: usize = 128;

/// A handle to an open Postgres connection.
///
/// Cheap to clone is *not* a goal in M0 — we hand out exactly one handle.
/// Pooling lives in M4.
#[derive(Debug)]
pub struct Session {
    tx: mpsc::Sender<Command>,
    /// Server-supplied parameter map captured during startup. Read-only
    /// snapshot; the driver task continues to absorb later
    /// `ParameterStatus` updates but doesn't expose them yet.
    params: ServerParams,
    /// Server's `BackendKeyData`. Used by out-of-band cancellation in a
    /// future milestone; stored now so we can document its presence.
    key_data: Option<driver::BackendKeyData>,
    /// Set on a clean shutdown to suppress the background reaper.
    closed: bool,
}

impl Session {
    /// Connect to Postgres using the supplied [`Config`].
    ///
    /// On success the returned `Session` owns a background task that drives
    /// the connection. Drop the `Session` (or call [`Session::close`]) to
    /// terminate the task and release the socket.
    pub async fn connect(config: Config) -> Result<Self> {
        startup::connect(config).await
    }

    /// Run a string through the simple-query protocol and collect every
    /// row from every result set in order.
    ///
    /// The returned shape is `Vec<ResultSet>` because a simple query
    /// string can contain multiple statements separated by `;`. Each inner
    /// `Vec<Vec<Option<Bytes>>>` is one statement's rows.
    ///
    /// `None` in a column position means SQL `NULL`. Bytes are exactly
    /// what the server returned in text format.
    ///
    /// # Note
    ///
    /// This is an internal/raw API in M0; the typed
    /// `Session::execute`/`Session::stream` surface arrives in M1.
    pub async fn simple_query_raw(&self, sql: &str) -> Result<Vec<RawRows>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Command::SimpleQuery {
                sql: sql.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::Closed)?;
        match reply_rx.await {
            Ok(result) => result,
            Err(_) => Err(Error::Closed),
        }
    }

    /// Run a [`Command`](QueryCommand) (DDL, INSERT/UPDATE/DELETE without
    /// `RETURNING`, etc.) over the extended protocol with `args` as the
    /// parameter tuple.
    ///
    /// Returns the affected-row count parsed out of the server's
    /// `CommandComplete` tag. DDL and other tags without a row count
    /// (e.g. `CREATE TABLE`) report `0`.
    ///
    /// Cancellation-safe: dropping the future leaves the in-flight command
    /// to run to completion on the driver task.
    pub async fn execute<A>(&self, cmd: &QueryCommand<A>, args: A) -> Result<u64> {
        let mut params: Vec<Option<Vec<u8>>> = Vec::with_capacity(cmd.encoder.oids().len());
        cmd.encoder.encode(&args, &mut params)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Command::ExtendedQuery {
                sql: cmd.sql.clone(),
                params,
                expected_columns: None,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::Closed)?;
        let outcome = match reply_rx.await {
            Ok(result) => result?,
            Err(_) => return Err(Error::Closed),
        };
        Ok(parse_affected_rows(outcome.command_tag.as_deref()))
    }

    /// Run a [`Query`] over the extended protocol and decode every row
    /// through the query's decoder.
    ///
    /// The driver validates that the server's `RowDescription` column count
    /// matches `query.n_columns()` and surfaces a mismatch as
    /// [`Error::ColumnAlignment`] before any rows are decoded.
    ///
    /// Returns all rows in a `Vec`. True row-by-row streaming arrives with
    /// pipelining in M2.
    ///
    /// Cancellation-safe: dropping the future leaves the in-flight query
    /// to run to completion on the driver task.
    pub async fn query<A, B>(&self, query: &Query<A, B>, args: A) -> Result<Vec<B>> {
        let mut params: Vec<Option<Vec<u8>>> = Vec::with_capacity(query.encoder.oids().len());
        query.encoder.encode(&args, &mut params)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Command::ExtendedQuery {
                sql: query.sql.clone(),
                params,
                expected_columns: Some(query.decoder.n_columns()),
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::Closed)?;
        let outcome = match reply_rx.await {
            Ok(result) => result?,
            Err(_) => return Err(Error::Closed),
        };
        let mut rows = Vec::with_capacity(outcome.rows.len());
        for cols in outcome.rows {
            rows.push(query.decoder.decode(&cols)?);
        }
        Ok(rows)
    }

    /// Send a `Terminate` and wait for the driver task to exit.
    ///
    /// Idempotent in the sense that calling it on a handle whose driver
    /// has already exited returns `Ok(())`.
    pub async fn close(mut self) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        // Best-effort: if the channel is already closed, the driver is
        // gone anyway.
        let _ = self.tx.send(Command::Close { reply: reply_tx }).await;
        // Bound the wait so a hung server doesn't stall a process exit.
        let outcome = tokio::time::timeout(Duration::from_secs(5), reply_rx).await;
        self.closed = true;
        match outcome {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Ok(()), // driver dropped its end without replying — acceptable
            Err(_) => Err(Error::Protocol(
                "Session::close timed out waiting for driver".into(),
            )),
        }
    }

    /// Snapshot of the server parameters reported during startup
    /// (`server_version`, `server_encoding`, etc).
    pub fn params(&self) -> &ServerParams {
        &self.params
    }

    /// Return the server-provided `BackendKeyData`, used for out-of-band
    /// cancellation (deferred past v0.1).
    pub fn backend_key(&self) -> Option<(i32, i32)> {
        self.key_data.map(|k| (k.process_id, k.secret_key))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.closed {
            // Send a Terminate via the command channel; the driver task will
            // observe the dropped sender and shut down. We don't await; this
            // is best-effort cleanup on the synchronous Drop path.
            let tx = self.tx.clone();
            // Try to send a Close without awaiting; if it fails the driver
            // is already gone and the socket is already closed.
            let _ = tx.try_send(Command::Close {
                reply: oneshot::channel().0,
            });
        }
    }
}

pub(crate) fn new_session(
    tx: mpsc::Sender<Command>,
    params: ServerParams,
    key_data: Option<driver::BackendKeyData>,
) -> Session {
    Session {
        tx,
        params,
        key_data,
        closed: false,
    }
}

pub(crate) const COMMAND_BUFFER_SIZE: usize = COMMAND_BUFFER;

/// Pull the row count out of a `CommandComplete` tag.
///
/// Postgres tag shapes (per protocol docs):
///   - `INSERT <oid> <count>` — count is the last token
///   - `UPDATE <count>`, `DELETE <count>`, `SELECT <count>`,
///     `MOVE <count>`, `FETCH <count>`, `COPY <count>` — count is the last token
///   - DDL like `CREATE TABLE` — no count, returns `0`
fn parse_affected_rows(tag: Option<&str>) -> u64 {
    let Some(tag) = tag else {
        return 0;
    };
    tag.split_whitespace()
        .next_back()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::parse_affected_rows;

    #[test]
    fn parses_insert_tag() {
        assert_eq!(parse_affected_rows(Some("INSERT 0 3")), 3);
    }

    #[test]
    fn parses_update_tag() {
        assert_eq!(parse_affected_rows(Some("UPDATE 5")), 5);
    }

    #[test]
    fn parses_delete_tag() {
        assert_eq!(parse_affected_rows(Some("DELETE 2")), 2);
    }

    #[test]
    fn ddl_without_count_is_zero() {
        assert_eq!(parse_affected_rows(Some("CREATE TABLE")), 0);
    }

    #[test]
    fn missing_tag_is_zero() {
        assert_eq!(parse_affected_rows(None), 0);
    }
}
