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

pub(crate) mod cache;
mod driver;
mod prepared;
mod startup;
mod stream;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, Mutex};

use crate::codec::Decoder;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::protocol::backend::RowField;
use crate::query::{Command as QueryCommand, Query};

use self::cache::{CacheKey, CachedStatement, StatementCache};
use self::stream::{begin_transaction, close_statement_best_effort, next_name, StreamConfig};

pub(crate) use driver::Command;
pub use driver::{RawRows, ServerParams};
pub use prepared::{PreparedCommand, PreparedQuery};
pub use stream::RowStream;

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
    /// Per-session prepared statement cache.
    cache: Arc<Mutex<StatementCache>>,
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
    /// `Session::execute`/`Session::query`/`Session::stream` surface sits
    /// alongside this raw escape hatch.
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
        let param_formats = cmd.encoder.format_codes().to_vec();
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Command::ExtendedQuery {
                sql: cmd.sql.clone(),
                params,
                param_formats,
                result_formats: Vec::new(),
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
    /// Returns all rows in a `Vec`. For row-by-row streaming with
    /// backpressure, see [`Session::stream`].
    ///
    /// Cancellation-safe: dropping the future leaves the in-flight query
    /// to run to completion on the driver task.
    pub async fn query<A, B>(&self, query: &Query<A, B>, args: A) -> Result<Vec<B>> {
        let mut params: Vec<Option<Vec<u8>>> = Vec::with_capacity(query.encoder.oids().len());
        query.encoder.encode(&args, &mut params)?;
        let param_formats = query.encoder.format_codes().to_vec();
        let result_formats = query.decoder.format_codes().to_vec();
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Command::ExtendedQuery {
                sql: query.sql.clone(),
                params,
                param_formats,
                result_formats,
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

    /// Run a [`Query`] over a server-side portal and stream decoded rows.
    ///
    /// Rows are fetched in bounded `Execute(max_rows)` batches. If the
    /// consumer is slow, the background fetch task naturally backpressures
    /// before requesting the next batch.
    ///
    /// This prepares a temporary server-side statement for the stream, binds a
    /// portal, and drives repeated `Execute` calls until the result set is
    /// exhausted. If you want to control how many rows each fetch requests, use
    /// [`Session::stream_with_batch_size`].
    ///
    /// Dropping the returned [`RowStream`] stops future batch fetches and
    /// best-effort closes the portal and temporary prepared statement.
    pub async fn stream<A, B>(&self, query: &Query<A, B>, args: A) -> Result<RowStream<B>>
    where
        B: Send + 'static,
    {
        self.stream_with_batch_size(query, args, stream::DEFAULT_BATCH_ROWS)
            .await
    }

    /// Like [`Session::stream`], but lets the caller choose the portal batch
    /// size.
    ///
    /// `batch_rows` must be greater than zero.
    pub async fn stream_with_batch_size<A, B>(
        &self,
        query: &Query<A, B>,
        args: A,
        batch_rows: usize,
    ) -> Result<RowStream<B>>
    where
        B: Send + 'static,
    {
        let mut params: Vec<Option<Vec<u8>>> = Vec::with_capacity(query.encoder.oids().len());
        query.encoder.encode(&args, &mut params)?;
        let param_formats = query.encoder.format_codes().to_vec();
        let stmt_name = next_name("babar_stream_stmt");
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Command::Prepare {
                name: stmt_name.clone(),
                sql: query.sql().to_string(),
                param_oids: query.encoder.oids().to_vec(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::Closed)?;
        let outcome = match reply_rx.await {
            Ok(result) => result?,
            Err(_) => return Err(Error::Closed),
        };

        if let Err(err) = validate_row_description(&query.decoder, &outcome.row_fields) {
            close_statement_best_effort(&self.tx, stmt_name).await;
            return Err(err);
        }

        if let Err(err) = begin_transaction(&self.tx).await {
            close_statement_best_effort(&self.tx, stmt_name).await;
            return Err(err);
        }

        RowStream::start(
            self.tx.clone(),
            StreamConfig {
                stmt_name,
                params,
                param_formats,
                decoder: Arc::clone(&query.decoder),
                batch_rows,
                close_statement_on_finish: true,
                close_transaction_on_finish: true,
            },
        )
        .await
    }

    /// Prepare a [`Query`] as a named server-side statement.
    ///
    /// Returns a [`PreparedQuery`] that can be executed multiple times without
    /// re-parsing. The statement is cached by SQL text + parameter OIDs, so
    /// calling `prepare_query` again with the same query returns another handle
    /// to the cached server-side statement.
    ///
    /// The server validates the SQL and reports parameter/column metadata
    /// at prepare time — schema mismatches surface here rather than at
    /// execute time.
    ///
    /// Lifecycle:
    ///
    /// 1. Prepare once with [`Session::prepare_query`].
    /// 2. Execute repeatedly with [`PreparedQuery::query`](super::PreparedQuery::query).
    /// 3. Close explicitly with [`PreparedQuery::close`](super::PreparedQuery::close)
    ///    when you need confirmation, or let the last handle drop for
    ///    best-effort cleanup.
    pub async fn prepare_query<A, B>(&self, query: &Query<A, B>) -> Result<PreparedQuery<A, B>>
    where
        A: 'static,
        B: 'static,
    {
        let param_oids = query.encoder.oids();
        let key = CacheKey::new(query.sql(), param_oids);

        // Fast path: cache hit.
        {
            let mut cache = self.cache.lock().await;
            if let Some(cached) = cache.checkout(&key) {
                return Ok(PreparedQuery::new(
                    cached.name.clone(),
                    query,
                    self.tx.clone(),
                    Arc::clone(&self.cache),
                    key,
                ));
            }
        }

        // Slow path: prepare on server.
        let name = self.cache.lock().await.next_name();
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Command::Prepare {
                name: name.clone(),
                sql: query.sql().to_string(),
                param_oids: param_oids.to_vec(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::Closed)?;
        let outcome = match reply_rx.await {
            Ok(result) => result?,
            Err(_) => return Err(Error::Closed),
        };

        // Schema validation: check column count and OIDs.
        if let Err(err) = validate_row_description(&query.decoder, &outcome.row_fields) {
            close_statement_best_effort(&self.tx, name).await;
            return Err(err);
        }

        // Cache the result.
        let cached = CachedStatement {
            name: name.clone(),
            param_oids: outcome.param_oids,
            row_fields: outcome.row_fields,
        };
        self.cache.lock().await.insert(key.clone(), cached);

        Ok(PreparedQuery::new(
            name,
            query,
            self.tx.clone(),
            Arc::clone(&self.cache),
            key,
        ))
    }

    /// Prepare a [`Command`](QueryCommand) as a named server-side statement.
    ///
    /// Similar to [`Session::prepare_query`] but for statements that don't
    /// return rows. The same cache and close/drop lifecycle rules apply.
    pub async fn prepare_command<A>(&self, cmd: &QueryCommand<A>) -> Result<PreparedCommand<A>>
    where
        A: 'static,
    {
        let param_oids = cmd.encoder.oids();
        let key = CacheKey::new(cmd.sql(), param_oids);

        {
            let mut cache = self.cache.lock().await;
            if let Some(cached) = cache.checkout(&key) {
                return Ok(PreparedCommand::new(
                    cached.name.clone(),
                    cmd,
                    self.tx.clone(),
                    Arc::clone(&self.cache),
                    key,
                ));
            }
        }

        let name = self.cache.lock().await.next_name();
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Command::Prepare {
                name: name.clone(),
                sql: cmd.sql().to_string(),
                param_oids: param_oids.to_vec(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| Error::Closed)?;
        let outcome = match reply_rx.await {
            Ok(result) => result?,
            Err(_) => return Err(Error::Closed),
        };

        let cached = CachedStatement {
            name: name.clone(),
            param_oids: outcome.param_oids,
            row_fields: outcome.row_fields,
        };
        self.cache.lock().await.insert(key.clone(), cached);

        Ok(PreparedCommand::new(
            name,
            cmd,
            self.tx.clone(),
            Arc::clone(&self.cache),
            key,
        ))
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
        cache: Arc::new(Mutex::new(StatementCache::new())),
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

fn validate_row_description<B>(
    decoder: &Arc<dyn Decoder<B> + Send + Sync>,
    row_fields: &[RowField],
) -> Result<()> {
    if row_fields.len() != decoder.n_columns() {
        return Err(Error::ColumnAlignment {
            expected: decoder.n_columns(),
            actual: row_fields.len(),
        });
    }

    let decoder_oids = decoder.oids();
    if !decoder_oids.is_empty() {
        for (position, (expected_oid, field)) in
            decoder_oids.iter().zip(row_fields.iter()).enumerate()
        {
            if *expected_oid != field.type_oid {
                return Err(Error::SchemaMismatch {
                    position,
                    expected_oid: *expected_oid,
                    actual_oid: field.type_oid,
                    column_name: field.name.clone(),
                });
            }
        }
    }

    Ok(())
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
