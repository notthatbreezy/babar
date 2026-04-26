use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_core::Stream;
use tokio::sync::{mpsc, oneshot};

use super::Command;
use crate::codec::Decoder;
use crate::error::{Error, Result};

pub(crate) const DEFAULT_BATCH_ROWS: usize = 128;

static STREAM_NAME_COUNTER: AtomicU64 = AtomicU64::new(1);

/// A row-by-row stream over a query result set.
///
/// Rows are fetched from Postgres in bounded portal batches and decoded on a
/// background task. Dropping the stream stops future batch fetches and
/// best-effort closes the server-side portal and temporary statement.
///
/// Constructed by [`Session::stream`](super::Session::stream) or
/// [`Session::stream_with_batch_size`](super::Session::stream_with_batch_size).
/// Use any `Stream` combinator ecosystem that works with `futures_core`.
pub struct RowStream<B> {
    rx: mpsc::Receiver<Result<B>>,
}

pub(crate) struct StreamConfig<B> {
    pub stmt_name: String,
    pub params: Vec<Option<Vec<u8>>>,
    pub param_formats: Vec<i16>,
    pub decoder: Arc<dyn Decoder<B> + Send + Sync>,
    pub batch_rows: usize,
    pub close_statement_on_finish: bool,
    pub close_transaction_on_finish: bool,
}

impl<B> RowStream<B> {
    pub(crate) async fn start(tx: mpsc::Sender<Command>, config: StreamConfig<B>) -> Result<Self>
    where
        B: Send + 'static,
    {
        let StreamConfig {
            stmt_name,
            params,
            param_formats,
            decoder,
            batch_rows,
            close_statement_on_finish,
            close_transaction_on_finish,
        } = config;

        let batch_rows_i32 = match i32::try_from(batch_rows) {
            Ok(value) if value > 0 => value,
            Ok(_) => {
                if close_statement_on_finish {
                    close_statement_best_effort(&tx, stmt_name).await;
                }
                if close_transaction_on_finish {
                    finish_transaction_best_effort(&tx).await;
                }
                return Err(Error::Config(
                    "stream batch size must be greater than zero".into(),
                ));
            }
            Err(_) => {
                if close_statement_on_finish {
                    close_statement_best_effort(&tx, stmt_name).await;
                }
                if close_transaction_on_finish {
                    finish_transaction_best_effort(&tx).await;
                }
                return Err(Error::Config("stream batch size exceeds i32::MAX".into()));
            }
        };

        let portal_name = next_name("babar_stream_portal");
        let (reply_tx, reply_rx) = oneshot::channel();
        let result_formats = decoder.format_codes().to_vec();
        if tx
            .send(Command::BindPortal {
                portal_name: portal_name.clone(),
                stmt_name: stmt_name.clone(),
                params,
                param_formats,
                result_formats,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            if close_statement_on_finish {
                close_statement_best_effort(&tx, stmt_name).await;
            }
            if close_transaction_on_finish {
                finish_transaction_best_effort(&tx).await;
            }
            return Err(Error::closed());
        }
        match reply_rx.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                if close_statement_on_finish {
                    close_statement_best_effort(&tx, stmt_name).await;
                }
                if close_transaction_on_finish {
                    finish_transaction_best_effort(&tx).await;
                }
                return Err(err);
            }
            Err(_) => {
                if close_statement_on_finish {
                    close_statement_best_effort(&tx, stmt_name).await;
                }
                if close_transaction_on_finish {
                    finish_transaction_best_effort(&tx).await;
                }
                return Err(Error::closed());
            }
        }

        let (row_tx, row_rx) = mpsc::channel(batch_rows);
        let statement_to_close = close_statement_on_finish.then_some(stmt_name);
        tokio::spawn(drive_stream(
            tx,
            portal_name,
            statement_to_close,
            decoder,
            batch_rows_i32,
            row_tx,
            close_transaction_on_finish,
        ));
        Ok(Self { rx: row_rx })
    }

    /// Stop receiving more rows.
    ///
    /// This is equivalent to dropping the stream, but can be useful when you
    /// want to end consumption early without dropping the binding immediately.
    pub fn close(&mut self) {
        self.rx.close();
    }
}

impl<B> Stream for RowStream<B> {
    type Item = Result<B>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl<B> std::fmt::Debug for RowStream<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RowStream").finish_non_exhaustive()
    }
}

async fn drive_stream<B>(
    tx: mpsc::Sender<Command>,
    portal_name: String,
    statement_name: Option<String>,
    decoder: Arc<dyn Decoder<B> + Send + Sync>,
    batch_rows: i32,
    row_tx: mpsc::Sender<Result<B>>,
    close_transaction_on_finish: bool,
) where
    B: Send + 'static,
{
    loop {
        if row_tx.is_closed() {
            break;
        }

        let (reply_tx, reply_rx) = oneshot::channel();
        if tx
            .send(Command::ExecutePortal {
                portal_name: portal_name.clone(),
                max_rows: batch_rows,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            let _ = row_tx.send(Err(Error::closed())).await;
            break;
        }

        let batch = match reply_rx.await {
            Ok(Ok(batch)) => batch,
            Ok(Err(err)) => {
                let _ = row_tx.send(Err(err)).await;
                break;
            }
            Err(_) => {
                let _ = row_tx.send(Err(Error::closed())).await;
                break;
            }
        };

        for columns in batch.rows {
            if row_tx.is_closed() {
                cleanup_stream(
                    &tx,
                    &portal_name,
                    statement_name.as_deref(),
                    close_transaction_on_finish,
                )
                .await;
                return;
            }

            let row = decoder.decode(&columns);
            let stop = row.is_err();
            if row_tx.send(row).await.is_err() {
                cleanup_stream(
                    &tx,
                    &portal_name,
                    statement_name.as_deref(),
                    close_transaction_on_finish,
                )
                .await;
                return;
            }
            if stop {
                cleanup_stream(
                    &tx,
                    &portal_name,
                    statement_name.as_deref(),
                    close_transaction_on_finish,
                )
                .await;
                return;
            }
        }

        if !batch.has_more {
            cleanup_stream(
                &tx,
                &portal_name,
                statement_name.as_deref(),
                close_transaction_on_finish,
            )
            .await;
            return;
        }
    }

    cleanup_stream(
        &tx,
        &portal_name,
        statement_name.as_deref(),
        close_transaction_on_finish,
    )
    .await;
}

async fn cleanup_stream(
    tx: &mpsc::Sender<Command>,
    portal_name: &str,
    statement_name: Option<&str>,
    close_transaction_on_finish: bool,
) {
    close_portal_best_effort(tx, portal_name.to_string()).await;
    if let Some(statement_name) = statement_name {
        close_statement_best_effort(tx, statement_name.to_string()).await;
    }
    if close_transaction_on_finish {
        finish_transaction_best_effort(tx).await;
    }
}

pub(crate) async fn close_statement_best_effort(tx: &mpsc::Sender<Command>, name: String) {
    let (reply_tx, reply_rx) = oneshot::channel();
    if tx
        .send(Command::CloseStatement {
            name,
            reply: reply_tx,
        })
        .await
        .is_ok()
    {
        let _ = reply_rx.await;
    }
}

pub(crate) async fn begin_transaction(tx: &mpsc::Sender<Command>) -> Result<()> {
    run_control_command(tx, "BEGIN").await
}

async fn finish_transaction_best_effort(tx: &mpsc::Sender<Command>) {
    let _ = run_control_command(tx, "COMMIT").await;
}

async fn close_portal_best_effort(tx: &mpsc::Sender<Command>, name: String) {
    let (reply_tx, reply_rx) = oneshot::channel();
    if tx
        .send(Command::ClosePortal {
            name,
            reply: reply_tx,
        })
        .await
        .is_ok()
    {
        let _ = reply_rx.await;
    }
}

pub(crate) async fn run_control_command(tx: &mpsc::Sender<Command>, sql: &str) -> Result<()> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(Command::ExtendedQuery {
        sql: sql.to_string(),
        params: Vec::new(),
        param_formats: Vec::new(),
        result_formats: Vec::new(),
        expected_columns: None,
        reply: reply_tx,
    })
    .await
    .map_err(|_| Error::closed())?;
    match reply_rx.await {
        Ok(result) => result.map(|_| ()),
        Err(_) => Err(Error::closed()),
    }
}

pub(crate) fn next_name(prefix: &str) -> String {
    let id = STREAM_NAME_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{id}")
}
