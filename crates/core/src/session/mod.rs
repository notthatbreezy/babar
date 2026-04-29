//! [`Session`] — the public, cancellation-safe handle to a Postgres connection.

pub(crate) mod cache;
mod driver;
mod prepared;
mod startup;
mod stream;
mod type_registry;

use std::borrow::Borrow;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::Instrument as _;

use crate::codec::Decoder;
use crate::config::Config;
use crate::copy::CopyIn;
use crate::error::{Error, Result};
use crate::protocol::backend::RowField;
use crate::query::{Command as QueryCommand, Query};
use crate::telemetry;
use crate::types::{Oid, Type};

use self::cache::{CacheKey, CachedStatement, StatementCache};
use self::stream::{begin_transaction, close_statement_best_effort, next_name, StreamConfig};
use self::type_registry::TypeRegistry;

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
    type_registry: Arc<Mutex<TypeRegistry>>,
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
            let bound = cmd
                .fragment
                .bind_runtime(&args)
                .map_err(|err| err.with_sql(cmd.sql(), cmd.origin()))?;
            let sql = bound.sql;
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(Command::ExtendedQuery {
                    sql: sql.clone(),
                    params: bound.params,
                    param_formats: bound.param_formats,
                    result_formats: Vec::new(),
                    expected_columns: None,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| Error::closed().with_sql(&sql, cmd.origin()))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(&sql, cmd.origin()))?,
                Err(_) => return Err(Error::closed().with_sql(&sql, cmd.origin())),
            };
            Ok(parse_affected_rows(outcome.command_tag.as_deref()))
        }
        .instrument(span)
        .await
    }

    /// Run a typed binary [`CopyIn`] and return the affected-row count.
    ///
    /// This is the dedicated bulk-ingest API for babar's limited COPY support:
    /// binary `COPY ... FROM STDIN` with rows supplied from an in-memory
    /// `IntoIterator` such as `Vec<T>`. `COPY TO`, text COPY, and CSV COPY are
    /// intentionally unsupported.
    pub async fn copy_in<T, I, R>(&self, copy: &CopyIn<T>, rows: I) -> Result<u64>
    where
        I: IntoIterator<Item = R>,
        R: Borrow<T>,
    {
        let data = copy.encode_rows(rows)?;
        self.copy_in_raw(copy.sql(), data).await
    }

    pub(crate) async fn copy_in_raw(&self, sql: &str, data: Vec<Bytes>) -> Result<u64> {
        let span = telemetry::execute_span(sql);
        async {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(Command::CopyIn {
                    sql: sql.to_string(),
                    data,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| Error::closed().with_sql(sql, None))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(sql, None))?,
                Err(_) => return Err(Error::closed().with_sql(sql, None)),
            };
            Ok(parse_affected_rows(Some(outcome.command_tag.as_str())))
        }
        .instrument(span)
        .await
    }

    /// Run a typed [`Query`] and decode every returned row.
    pub async fn query<A, B>(&self, query: &Query<A, B>, args: A) -> Result<Vec<B>> {
        let span = telemetry::execute_span(query.sql());
        async {
            let bound = query
                .fragment
                .bind_runtime(&args)
                .map_err(|err| err.with_sql(query.sql(), query.origin()))?;
            let sql = bound.sql;
            let result_formats = query.decoder.format_codes().to_vec();
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(Command::ExtendedQuery {
                    sql: sql.clone(),
                    params: bound.params,
                    param_formats: bound.param_formats,
                    result_formats,
                    expected_columns: Some(query.decoder.n_columns()),
                    reply: reply_tx,
                })
                .await
                .map_err(|_| Error::closed().with_sql(&sql, query.origin()))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(&sql, query.origin()))?,
                Err(_) => return Err(Error::closed().with_sql(&sql, query.origin())),
            };
            let mut rows = Vec::with_capacity(outcome.rows.len());
            for cols in outcome.rows {
                rows.push(
                    query
                        .decoder
                        .decode(&cols)
                        .map_err(|err| err.with_sql(&sql, query.origin()))?,
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
            let bound = query
                .fragment
                .bind_runtime(&args)
                .map_err(|err| err.with_sql(query.sql(), query.origin()))?;
            let sql = bound.sql;
            let stmt_name = next_name("babar_stream_stmt");
            let param_oids = self.resolve_type_oids(&bound.param_types).await?;
            let output_oids = self.resolve_type_oids(query.decoder.types()).await?;
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(Command::Prepare {
                    name: stmt_name.clone(),
                    sql: sql.clone(),
                    param_oids,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| Error::closed().with_sql(&sql, query.origin()))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(&sql, query.origin()))?,
                Err(_) => return Err(Error::closed().with_sql(&sql, query.origin())),
            };

            if let Err(err) =
                validate_row_description(&query.decoder, &outcome.row_fields, &output_oids)
            {
                close_statement_best_effort(&self.tx, stmt_name).await;
                return Err(err.with_sql(&sql, query.origin()));
            }

            if let Err(err) = begin_transaction(&self.tx).await {
                close_statement_best_effort(&self.tx, stmt_name).await;
                return Err(err.with_sql(&sql, query.origin()));
            }

            RowStream::start(
                self.tx.clone(),
                StreamConfig {
                    stmt_name,
                    params: bound.params,
                    param_formats: bound.param_formats,
                    decoder: Arc::clone(&query.decoder),
                    batch_rows,
                    close_statement_on_finish: true,
                    close_transaction_on_finish: true,
                },
            )
            .await
            .map_err(|err| err.with_sql(&sql, query.origin()))
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
            if query.fragment.dynamic.is_some() {
                return Err(Error::Codec(
                    "queries with runtime-dependent optional typed_query! SQL cannot be prepared; call Session::query or Session::stream with arguments instead".into(),
                )
                .with_sql(query.sql(), query.origin()));
            }
            let param_types = query.fragment.param_types();
            let key = CacheKey::new(query.sql(), param_types);

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
            let param_oids = self.resolve_type_oids(param_types).await?;
            let output_oids = self.resolve_type_oids(query.decoder.types()).await?;
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(Command::Prepare {
                    name: name.clone(),
                    sql: query.sql().to_string(),
                    param_oids,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| Error::closed().with_sql(query.sql(), query.origin()))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(query.sql(), query.origin()))?,
                Err(_) => return Err(Error::closed().with_sql(query.sql(), query.origin())),
            };

            if let Err(err) =
                validate_row_description(&query.decoder, &outcome.row_fields, &output_oids)
            {
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
            if cmd.fragment.dynamic.is_some() {
                return Err(Error::Codec(
                    "commands with runtime-dependent SQL cannot be prepared".into(),
                )
                .with_sql(cmd.sql(), cmd.origin()));
            }
            let param_types = cmd.fragment.param_types();
            let key = CacheKey::new(cmd.sql(), param_types);

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
            let param_oids = self.resolve_type_oids(param_types).await?;
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(Command::Prepare {
                    name: name.clone(),
                    sql: cmd.sql().to_string(),
                    param_oids,
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

    async fn resolve_type_oids(&self, declared: &[Type]) -> Result<Vec<Oid>> {
        let mut resolved = Vec::with_capacity(declared.len());

        for ty in declared {
            if ty.is_resolved() {
                resolved.push(ty.oid());
                continue;
            }

            if let Some(oid) = self.type_registry.lock().await.get(*ty) {
                resolved.push(oid);
                continue;
            }

            let oid = self.resolve_dynamic_type(*ty).await?;
            self.type_registry.lock().await.insert(*ty, oid);
            resolved.push(oid);
        }

        Ok(resolved)
    }

    async fn resolve_dynamic_type(&self, ty: Type) -> Result<Oid> {
        let sql = if let Some(extension) = ty.extension_name() {
            format!(
                "SELECT t.oid::text \
                 FROM pg_type AS t \
                 JOIN pg_namespace AS n ON n.oid = t.typnamespace \
                 JOIN pg_extension AS e ON e.extnamespace = n.oid \
                 WHERE e.extname = '{}' AND t.typname = '{}'",
                escape_sql_literal(extension),
                escape_sql_literal(ty.name()),
            )
        } else {
            format!(
                "SELECT to_regtype('{}')::oid::text",
                escape_sql_literal(ty.name()),
            )
        };

        let rows = self.simple_query_raw(&sql).await?;
        let raw = rows
            .first()
            .and_then(|rowset| rowset.first())
            .and_then(|row| row.first())
            .and_then(|cell| cell.as_deref())
            .ok_or_else(|| {
                Error::Codec(format!(
                    "could not resolve PostgreSQL type \"{}\"{}",
                    ty.name(),
                    ty.extension_name()
                        .map(|ext| format!(" from extension \"{ext}\""))
                        .unwrap_or_default()
                ))
            })?;

        let text = std::str::from_utf8(raw).map_err(|_| {
            Error::Codec(format!(
                "type resolution for \"{}\" returned non-UTF-8",
                ty.name()
            ))
        })?;
        text.parse::<Oid>().map_err(|_| {
            Error::Codec(format!(
                "type resolution for \"{}\" returned invalid OID {text:?}",
                ty.name()
            ))
        })
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
        type_registry: Arc::new(Mutex::new(TypeRegistry::default())),
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
    expected_oids: &[Oid],
) -> Result<()> {
    if row_fields.len() != decoder.n_columns() {
        return Err(Error::ColumnAlignment {
            expected: decoder.n_columns(),
            actual: row_fields.len(),
            sql: None,
            origin: None,
        });
    }

    if !expected_oids.is_empty() {
        for (position, (expected_oid, field)) in
            expected_oids.iter().zip(row_fields.iter()).enumerate()
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

fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
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
