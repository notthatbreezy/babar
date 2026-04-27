//! `PreparedQuery` and `PreparedCommand` handles.

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::Instrument as _;

use super::cache::{CacheKey, StatementCache};
use super::{parse_affected_rows, Command};
use crate::codec::{Decoder, Encoder};
use crate::error::{Error, Result};
use crate::query::{Command as QueryCommand, Fragment, Origin, Query};
use crate::telemetry;
use crate::types::{Oid, Type};

struct PreparedStatement<A> {
    name: String,
    fragment: Fragment<A>,
    tx: mpsc::Sender<Command>,
    cache: Arc<Mutex<StatementCache>>,
    key: CacheKey,
    closed: bool,
}

impl<A> PreparedStatement<A> {
    fn new(
        name: String,
        fragment: Fragment<A>,
        tx: mpsc::Sender<Command>,
        cache: Arc<Mutex<StatementCache>>,
        key: CacheKey,
    ) -> Self {
        Self {
            name,
            fragment,
            tx,
            cache,
            key,
            closed: false,
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn sql(&self) -> &str {
        self.fragment.sql()
    }

    fn origin(&self) -> Option<Origin> {
        self.fragment.origin()
    }

    fn param_oids(&self) -> &'static [Oid] {
        self.fragment.param_oids()
    }

    fn param_types(&self) -> &'static [Type] {
        self.fragment.param_types()
    }

    fn encode(&self, args: &A) -> Result<Vec<Option<Vec<u8>>>> {
        let mut params = Vec::with_capacity(self.fragment.encoder.oids().len());
        self.fragment.encoder.encode(args, &mut params)?;
        Ok(params)
    }

    fn param_formats(&self) -> Vec<i16> {
        self.fragment.encoder.format_codes().to_vec()
    }

    fn closed_error(&self) -> Error {
        Error::closed().with_sql(self.sql(), self.origin())
    }

    async fn close(&mut self) -> Result<()> {
        self.closed = true;
        if self.cache.lock().await.release_handle(&self.key).is_none() {
            return Ok(());
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Command::CloseStatement {
                name: self.name.clone(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| self.closed_error())?;
        match reply_rx.await {
            Ok(result) => result.map_err(|err| err.with_sql(self.sql(), self.origin())),
            Err(_) => Err(self.closed_error()),
        }
    }

    fn schedule_close_on_drop(&mut self) {
        if self.closed {
            return;
        }

        let tx = self.tx.clone();
        let name = self.name.clone();
        let cache = Arc::clone(&self.cache);
        let key = self.key.clone();
        tokio::spawn(async move {
            if cache.lock().await.release_handle(&key).is_none() {
                return;
            }
            let (reply_tx, _reply_rx) = oneshot::channel();
            let _ = tx
                .send(Command::CloseStatement {
                    name,
                    reply: reply_tx,
                })
                .await;
        });
    }
}

/// A prepared statement that returns rows.
pub struct PreparedQuery<A, B> {
    statement: PreparedStatement<A>,
    decoder: Arc<dyn Decoder<B> + Send + Sync>,
}

impl<A, B> PreparedQuery<A, B> {
    pub(super) fn new(
        name: String,
        query: &Query<A, B>,
        tx: mpsc::Sender<Command>,
        cache: Arc<Mutex<StatementCache>>,
        key: CacheKey,
    ) -> Self {
        Self {
            statement: PreparedStatement::new(name, query.fragment.clone(), tx, cache, key),
            decoder: Arc::clone(&query.decoder),
        }
    }

    /// Execute the prepared query with the given arguments and decode all rows.
    pub async fn query(&self, args: A) -> Result<Vec<B>> {
        let span = telemetry::execute_span(self.sql());
        async {
            let params = self
                .statement
                .encode(&args)
                .map_err(|err| err.with_sql(self.sql(), self.origin()))?;
            let param_formats = self.statement.param_formats();
            let result_formats = self.decoder.format_codes().to_vec();
            let (reply_tx, reply_rx) = oneshot::channel();
            self.statement
                .tx
                .send(Command::ExecutePrepared {
                    stmt_name: self.statement.name.clone(),
                    params,
                    param_formats,
                    result_formats,
                    expected_columns: Some(self.decoder.n_columns()),
                    reply: reply_tx,
                })
                .await
                .map_err(|_| self.statement.closed_error())?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(self.sql(), self.origin()))?,
                Err(_) => return Err(self.statement.closed_error()),
            };
            let mut rows = Vec::with_capacity(outcome.rows.len());
            for cols in outcome.rows {
                rows.push(
                    self.decoder
                        .decode(&cols)
                        .map_err(|err| err.with_sql(self.sql(), self.origin()))?,
                );
            }
            Ok(rows)
        }
        .instrument(span)
        .await
    }

    /// The server-side statement name.
    pub fn name(&self) -> &str {
        self.statement.name()
    }

    /// SQL text exactly as it was prepared.
    pub fn sql(&self) -> &str {
        self.statement.sql()
    }

    /// Macro callsite captured by [`crate::sql!`], when available.
    pub fn origin(&self) -> Option<Origin> {
        self.statement.origin()
    }

    /// Postgres OIDs the encoder declares, in placeholder order.
    pub fn param_oids(&self) -> &'static [Oid] {
        self.statement.param_oids()
    }

    /// Postgres type metadata the encoder declares, in placeholder order.
    pub fn param_types(&self) -> &'static [Type] {
        self.statement.param_types()
    }

    /// Postgres OIDs the decoder expects, in column order.
    pub fn output_oids(&self) -> &'static [Oid] {
        self.decoder.oids()
    }

    /// Postgres type metadata the decoder expects, in column order.
    pub fn output_types(&self) -> &'static [Type] {
        self.decoder.types()
    }

    /// Number of columns the decoder expects.
    pub fn n_columns(&self) -> usize {
        self.decoder.n_columns()
    }

    /// Explicitly close this prepared statement.
    pub async fn close(mut self) -> Result<()> {
        self.statement.close().await
    }
}

impl<A, B> Drop for PreparedQuery<A, B> {
    fn drop(&mut self) {
        self.statement.schedule_close_on_drop();
    }
}

impl<A, B> std::fmt::Debug for PreparedQuery<A, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedQuery")
            .field("name", &self.name())
            .field("sql", &self.sql())
            .finish_non_exhaustive()
    }
}

/// A prepared statement that does not return rows.
pub struct PreparedCommand<A> {
    statement: PreparedStatement<A>,
}

impl<A> PreparedCommand<A> {
    pub(super) fn new(
        name: String,
        cmd: &QueryCommand<A>,
        tx: mpsc::Sender<Command>,
        cache: Arc<Mutex<StatementCache>>,
        key: CacheKey,
    ) -> Self {
        Self {
            statement: PreparedStatement::new(name, cmd.fragment.clone(), tx, cache, key),
        }
    }

    /// Execute the prepared command with the given arguments.
    pub async fn execute(&self, args: A) -> Result<u64> {
        let span = telemetry::execute_span(self.sql());
        async {
            let params = self
                .statement
                .encode(&args)
                .map_err(|err| err.with_sql(self.sql(), self.origin()))?;
            let param_formats = self.statement.param_formats();
            let (reply_tx, reply_rx) = oneshot::channel();
            self.statement
                .tx
                .send(Command::ExecutePrepared {
                    stmt_name: self.statement.name.clone(),
                    params,
                    param_formats,
                    result_formats: Vec::new(),
                    expected_columns: None,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| self.statement.closed_error())?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(self.sql(), self.origin()))?,
                Err(_) => return Err(self.statement.closed_error()),
            };
            Ok(parse_affected_rows(outcome.command_tag.as_deref()))
        }
        .instrument(span)
        .await
    }

    /// The server-side statement name.
    pub fn name(&self) -> &str {
        self.statement.name()
    }

    /// SQL text exactly as it was prepared.
    pub fn sql(&self) -> &str {
        self.statement.sql()
    }

    /// Macro callsite captured by [`crate::sql!`], when available.
    pub fn origin(&self) -> Option<Origin> {
        self.statement.origin()
    }

    /// Postgres OIDs the encoder declares, in placeholder order.
    pub fn param_oids(&self) -> &'static [Oid] {
        self.statement.param_oids()
    }

    /// Postgres type metadata the encoder declares, in placeholder order.
    pub fn param_types(&self) -> &'static [Type] {
        self.statement.param_types()
    }

    /// Explicitly close this prepared statement.
    pub async fn close(mut self) -> Result<()> {
        self.statement.close().await
    }
}

impl<A> Drop for PreparedCommand<A> {
    fn drop(&mut self) {
        self.statement.schedule_close_on_drop();
    }
}

impl<A> std::fmt::Debug for PreparedCommand<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedCommand")
            .field("name", &self.name())
            .field("sql", &self.sql())
            .finish_non_exhaustive()
    }
}
