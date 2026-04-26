//! [`Session`] — the public, cancellation-safe handle to a Postgres connection.

pub(crate) mod cache;
mod driver;
mod prepared;
mod startup;
mod stream;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::Instrument as _;

use crate::codec::Decoder;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::protocol::backend::RowField;
use crate::query::{Command as QueryCommand, Query};
use crate::telemetry;

use self::cache::{CacheKey, CachedStatement, StatementCache};
use self::stream::{begin_transaction, close_statement_best_effort, next_name, StreamConfig};

pub(crate) use driver::Command;
pub use driver::{RawRows, ServerParams};
pub use prepared::{PreparedCommand, PreparedQuery};
pub use stream::RowStream;

const COMMAND_BUFFER: usize = 128;

/// A handle to an open Postgres connection.
#[derive(Debug)]
pub struct Session {
    tx: mpsc::Sender<Command>,
    params: ServerParams,
    key_data: Option<driver::BackendKeyData>,
    cache: Arc<Mutex<StatementCache>>,
    closed: bool,
    state: Arc<driver::DriverState>,
}

impl Session {
    /// Connect to Postgres using the supplied [`Config`].
    pub async fn connect(config: Config) -> Result<Self> {
        startup::connect(config, false).await
    }

    pub(crate) async fn connect_pooled(config: Config) -> Result<Self> {
        startup::connect(config, true).await
    }

    /// Run a string through the simple-query protocol and collect every result
    /// set in order.
    pub async fn simple_query_raw(&self, sql: &str) -> Result<Vec<RawRows>> {
        let span = telemetry::execute_span(sql);
        async {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(Command::SimpleQuery {
                    sql: sql.to_string(),
                    reply: reply_tx,
                })
                .await
                .map_err(|_| Error::closed().with_sql(sql, None))?;
            match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(sql, None)),
                Err(_) => Err(Error::closed().with_sql(sql, None)),
            }
        }
        .instrument(span)
        .await
    }

    /// Run a typed [`Command`](QueryCommand) and return the affected-row count.
    pub async fn execute<A>(&self, cmd: &QueryCommand<A>, args: A) -> Result<u64> {
        let span = telemetry::execute_span(cmd.sql());
        async {
            let mut params: Vec<Option<Vec<u8>>> = Vec::with_capacity(cmd.encoder.oids().len());
            cmd.encoder
                .encode(&args, &mut params)
                .map_err(|err| err.with_sql(cmd.sql(), cmd.origin()))?;
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
                .map_err(|_| Error::closed().with_sql(cmd.sql(), cmd.origin()))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(cmd.sql(), cmd.origin()))?,
                Err(_) => return Err(Error::closed().with_sql(cmd.sql(), cmd.origin())),
            };
            Ok(parse_affected_rows(outcome.command_tag.as_deref()))
        }
        .instrument(span)
        .await
    }

    /// Run a typed [`Query`] and decode every returned row.
    pub async fn query<A, B>(&self, query: &Query<A, B>, args: A) -> Result<Vec<B>> {
        let span = telemetry::execute_span(query.sql());
        async {
            let mut params: Vec<Option<Vec<u8>>> = Vec::with_capacity(query.encoder.oids().len());
            query
                .encoder
                .encode(&args, &mut params)
                .map_err(|err| err.with_sql(query.sql(), query.origin()))?;
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
                .map_err(|_| Error::closed().with_sql(query.sql(), query.origin()))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(query.sql(), query.origin()))?,
                Err(_) => return Err(Error::closed().with_sql(query.sql(), query.origin())),
            };
            let mut rows = Vec::with_capacity(outcome.rows.len());
            for cols in outcome.rows {
                rows.push(
                    query
                        .decoder
                        .decode(&cols)
                        .map_err(|err| err.with_sql(query.sql(), query.origin()))?,
                );
            }
            Ok(rows)
        }
        .instrument(span)
        .await
    }

    /// Run a [`Query`] over a server-side portal and stream decoded rows.
    pub async fn stream<A, B>(&self, query: &Query<A, B>, args: A) -> Result<RowStream<B>>
    where
        B: Send + 'static,
    {
        self.stream_with_batch_size(query, args, stream::DEFAULT_BATCH_ROWS)
            .await
    }

    /// Like [`Session::stream`], but lets the caller choose the portal batch
    /// size.
    pub async fn stream_with_batch_size<A, B>(
        &self,
        query: &Query<A, B>,
        args: A,
        batch_rows: usize,
    ) -> Result<RowStream<B>>
    where
        B: Send + 'static,
    {
        let span = telemetry::execute_span(query.sql());
        async {
            let mut params: Vec<Option<Vec<u8>>> = Vec::with_capacity(query.encoder.oids().len());
            query
                .encoder
                .encode(&args, &mut params)
                .map_err(|err| err.with_sql(query.sql(), query.origin()))?;
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
                .map_err(|_| Error::closed().with_sql(query.sql(), query.origin()))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(query.sql(), query.origin()))?,
                Err(_) => return Err(Error::closed().with_sql(query.sql(), query.origin())),
            };

            if let Err(err) = validate_row_description(&query.decoder, &outcome.row_fields) {
                close_statement_best_effort(&self.tx, stmt_name).await;
                return Err(err.with_sql(query.sql(), query.origin()));
            }

            if let Err(err) = begin_transaction(&self.tx).await {
                close_statement_best_effort(&self.tx, stmt_name).await;
                return Err(err.with_sql(query.sql(), query.origin()));
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
            .map_err(|err| err.with_sql(query.sql(), query.origin()))
        }
        .instrument(span)
        .await
    }

    /// Prepare a [`Query`] as a named server-side statement.
    pub async fn prepare_query<A, B>(&self, query: &Query<A, B>) -> Result<PreparedQuery<A, B>>
    where
        A: 'static,
        B: 'static,
    {
        let span = telemetry::prepare_span(query.sql());
        async {
            let param_oids = query.encoder.oids();
            let key = CacheKey::new(query.sql(), param_oids);

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
                .map_err(|_| Error::closed().with_sql(query.sql(), query.origin()))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(query.sql(), query.origin()))?,
                Err(_) => return Err(Error::closed().with_sql(query.sql(), query.origin())),
            };

            if let Err(err) = validate_row_description(&query.decoder, &outcome.row_fields) {
                close_statement_best_effort(&self.tx, name).await;
                return Err(err.with_sql(query.sql(), query.origin()));
            }

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
        .instrument(span)
        .await
    }

    /// Prepare a [`Command`](QueryCommand) as a named server-side statement.
    pub async fn prepare_command<A>(&self, cmd: &QueryCommand<A>) -> Result<PreparedCommand<A>>
    where
        A: 'static,
    {
        let span = telemetry::prepare_span(cmd.sql());
        async {
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
                .map_err(|_| Error::closed().with_sql(cmd.sql(), cmd.origin()))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(cmd.sql(), cmd.origin()))?,
                Err(_) => return Err(Error::closed().with_sql(cmd.sql(), cmd.origin())),
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
        .instrument(span)
        .await
    }

    /// Send a `Terminate` and wait for the driver task to exit.
    pub async fn close(mut self) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.tx.send(Command::Close { reply: reply_tx }).await;
        let outcome = tokio::time::timeout(Duration::from_secs(5), reply_rx).await;
        self.closed = true;
        match outcome {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Ok(()),
            Err(_) => Err(Error::Protocol(
                "Session::close timed out waiting for driver".into(),
            )),
        }
    }

    /// Snapshot of the server parameters reported during startup.
    pub fn params(&self) -> &ServerParams {
        &self.params
    }

    /// Return the server-provided `BackendKeyData`.
    pub fn backend_key(&self) -> Option<(i32, i32)> {
        self.key_data.map(|k| (k.process_id, k.secret_key))
    }

    pub(crate) fn command_tx(&self) -> mpsc::Sender<Command> {
        self.tx.clone()
    }

    pub(crate) async fn run_control_command(&self, sql: &str) -> Result<()> {
        stream::run_control_command(&self.tx, sql).await
    }

    pub(crate) fn transaction_status(&self) -> u8 {
        self.state.transaction_status()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.closed {
            let tx = self.tx.clone();
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
    state: Arc<driver::DriverState>,
    retain_prepared_statements: bool,
) -> Session {
    Session {
        tx,
        params,
        key_data,
        cache: Arc::new(Mutex::new(StatementCache::new(retain_prepared_statements))),
        state,
        closed: false,
    }
}

pub(crate) const COMMAND_BUFFER_SIZE: usize = COMMAND_BUFFER;

pub(crate) async fn run_control_command_with_tx(
    tx: &mpsc::Sender<Command>,
    sql: &str,
) -> Result<()> {
    stream::run_control_command(tx, sql).await
}

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
            sql: None,
            origin: None,
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
                    sql: None,
                    origin: None,
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
