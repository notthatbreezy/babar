//! `PreparedQuery` and `PreparedCommand` handles.

use std::marker::PhantomData;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::Instrument as _;

use super::cache::{CacheKey, StatementCache};
use super::{parse_affected_rows, Command};
use crate::codec::{Decoder, Encoder};
use crate::error::{Error, Result};
use crate::query::{Command as QueryCommand, Origin, Query};
use crate::telemetry;

/// A prepared statement that returns rows.
pub struct PreparedQuery<A, B> {
    name: String,
    sql: String,
    origin: Option<Origin>,
    encoder: Arc<dyn Encoder<A> + Send + Sync>,
    decoder: Arc<dyn Decoder<B> + Send + Sync>,
    tx: mpsc::Sender<Command>,
    cache: Arc<Mutex<StatementCache>>,
    key: CacheKey,
    closed: bool,
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
            name,
            sql: query.sql().to_string(),
            origin: query.origin(),
            encoder: Arc::clone(&query.encoder),
            decoder: Arc::clone(&query.decoder),
            tx,
            cache,
            key,
            closed: false,
        }
    }

    /// Execute the prepared query with the given arguments and decode all rows.
    pub async fn query(&self, args: A) -> Result<Vec<B>> {
        let span = telemetry::execute_span(&self.sql);
        async {
            let mut params: Vec<Option<Vec<u8>>> = Vec::with_capacity(self.encoder.oids().len());
            self.encoder
                .encode(&args, &mut params)
                .map_err(|err| err.with_sql(&self.sql, self.origin))?;
            let param_formats = self.encoder.format_codes().to_vec();
            let result_formats = self.decoder.format_codes().to_vec();
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(Command::ExecutePrepared {
                    stmt_name: self.name.clone(),
                    params,
                    param_formats,
                    result_formats,
                    expected_columns: Some(self.decoder.n_columns()),
                    reply: reply_tx,
                })
                .await
                .map_err(|_| Error::closed().with_sql(&self.sql, self.origin))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(&self.sql, self.origin))?,
                Err(_) => return Err(Error::closed().with_sql(&self.sql, self.origin)),
            };
            let mut rows = Vec::with_capacity(outcome.rows.len());
            for cols in outcome.rows {
                rows.push(
                    self.decoder
                        .decode(&cols)
                        .map_err(|err| err.with_sql(&self.sql, self.origin))?,
                );
            }
            Ok(rows)
        }
        .instrument(span)
        .await
    }

    /// The server-side statement name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Explicitly close this prepared statement.
    pub async fn close(mut self) -> Result<()> {
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
            .map_err(|_| Error::closed().with_sql(&self.sql, self.origin))?;
        match reply_rx.await {
            Ok(result) => result.map_err(|err| err.with_sql(&self.sql, self.origin)),
            Err(_) => Err(Error::closed().with_sql(&self.sql, self.origin)),
        }
    }
}

impl<A, B> Drop for PreparedQuery<A, B> {
    fn drop(&mut self) {
        if !self.closed {
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
}

impl<A, B> std::fmt::Debug for PreparedQuery<A, B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedQuery")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// A prepared statement that does not return rows.
pub struct PreparedCommand<A> {
    name: String,
    sql: String,
    origin: Option<Origin>,
    encoder: Arc<dyn Encoder<A> + Send + Sync>,
    tx: mpsc::Sender<Command>,
    cache: Arc<Mutex<StatementCache>>,
    key: CacheKey,
    closed: bool,
    _marker: PhantomData<fn(A)>,
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
            name,
            sql: cmd.sql().to_string(),
            origin: cmd.origin(),
            encoder: Arc::clone(&cmd.encoder),
            tx,
            cache,
            key,
            closed: false,
            _marker: PhantomData,
        }
    }

    /// Execute the prepared command with the given arguments.
    pub async fn execute(&self, args: A) -> Result<u64> {
        let span = telemetry::execute_span(&self.sql);
        async {
            let mut params: Vec<Option<Vec<u8>>> = Vec::with_capacity(self.encoder.oids().len());
            self.encoder
                .encode(&args, &mut params)
                .map_err(|err| err.with_sql(&self.sql, self.origin))?;
            let param_formats = self.encoder.format_codes().to_vec();
            let (reply_tx, reply_rx) = oneshot::channel();
            self.tx
                .send(Command::ExecutePrepared {
                    stmt_name: self.name.clone(),
                    params,
                    param_formats,
                    result_formats: Vec::new(),
                    expected_columns: None,
                    reply: reply_tx,
                })
                .await
                .map_err(|_| Error::closed().with_sql(&self.sql, self.origin))?;
            let outcome = match reply_rx.await {
                Ok(result) => result.map_err(|err| err.with_sql(&self.sql, self.origin))?,
                Err(_) => return Err(Error::closed().with_sql(&self.sql, self.origin)),
            };
            Ok(parse_affected_rows(outcome.command_tag.as_deref()))
        }
        .instrument(span)
        .await
    }

    /// The server-side statement name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Explicitly close this prepared statement.
    pub async fn close(mut self) -> Result<()> {
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
            .map_err(|_| Error::closed().with_sql(&self.sql, self.origin))?;
        match reply_rx.await {
            Ok(result) => result.map_err(|err| err.with_sql(&self.sql, self.origin)),
            Err(_) => Err(Error::closed().with_sql(&self.sql, self.origin)),
        }
    }
}

impl<A> Drop for PreparedCommand<A> {
    fn drop(&mut self) {
        if !self.closed {
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
}

impl<A> std::fmt::Debug for PreparedCommand<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedCommand")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
