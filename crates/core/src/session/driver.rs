//! Background driver task.
//!
//! The driver owns the [`tokio::net::TcpStream`], the receive side of the
//! command channel, and a small state machine. It reads frames with
//! [`tokio_util::codec::FramedRead`] and writes frames directly to the
//! write half. Per-command reply channels are stored in a small FIFO so
//! responses can be matched to the originating command in arrival order.
//!
//! ## Invariants
//!
//! - Exactly one task reads the socket; exactly one task writes to it
//!   (the same one). No interleaving.
//! - A command is acknowledged on its `oneshot` only after a
//!   `ReadyForQuery` is observed. This serializes the protocol — M0 does
//!   not pipeline. (Pipelining lands in M2.)
//! - If the socket dies, every pending reply channel is dropped
//!   (`Err(RecvError)`), which surfaces to callers as [`Error::Closed`].
//! - When the [`Session`] handle is dropped, the command channel closes,
//!   the driver finishes any in-flight command, sends `Terminate`, and
//!   exits.
//!
//! ## Drop safety
//!
//! Dropping the future returned by `Session::simple_query_raw` only drops
//! the receiver of the corresponding `oneshot`. The driver still drives
//! the response to completion and discards the result. The next command
//! after the cancelled one starts from a known `Idle` state.
//!
//! [`Session`]: crate::Session

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use futures_util::stream::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::FramedRead;
use tracing::{debug, trace, warn};

use crate::error::{Error, Result};
use crate::protocol::backend::{BackendMessage, RowField};
use crate::protocol::codec::BackendCodec;
use crate::protocol::frontend;

/// Rows of one result set in the simple-query protocol: each row is a vector
/// of optional column bytes (`None` is SQL `NULL`).
///
/// Public alias for the M0 raw API. The typed `Session::execute` and
/// `Session::stream` surface arriving in M1 will replace this shape with
/// codec-driven rows.
pub type RawRows = Vec<Vec<Option<Bytes>>>;

/// One simple-query string can carry multiple statements; each produces a
/// `RawRows`.
type SimpleQueryReply = oneshot::Sender<Result<Vec<RawRows>>>;

/// Result of a single round-trip through the extended protocol.
#[derive(Debug)]
pub(crate) struct ExtendedOutcome {
    /// Raw rows the server returned (text format). Empty for commands
    /// that produce no rows (DDL, modifications without RETURNING).
    pub rows: RawRows,
    /// Server's `CommandComplete` tag (e.g. `"INSERT 0 3"`).
    pub command_tag: Option<String>,
}

/// Result of a `Prepare` round-trip: server-confirmed parameter OIDs +
/// row metadata.
#[derive(Debug, Clone)]
pub(crate) struct PrepareOutcome {
    /// Parameter OIDs the server inferred (from `ParameterDescription`).
    pub param_oids: Vec<u32>,
    /// Column metadata (from `RowDescription`), empty if no rows.
    pub row_fields: Vec<RowField>,
}

/// Result of a portal `Execute` batch.
#[derive(Debug)]
pub(crate) struct PortalBatch {
    /// Rows returned in this batch.
    pub rows: RawRows,
    /// `true` if the portal still has rows (server sent `PortalSuspended`).
    /// `false` if the portal is exhausted (`CommandComplete`).
    pub has_more: bool,
}

type ExtendedReply = oneshot::Sender<Result<ExtendedOutcome>>;
type PrepareReply = oneshot::Sender<Result<PrepareOutcome>>;

/// One unit of work the driver task accepts.
pub enum Command {
    /// Run `sql` through the simple-query protocol. Reply with the rows of
    /// every result set in order. `None` means SQL NULL.
    SimpleQuery {
        sql: String,
        reply: SimpleQueryReply,
    },
    /// Run `sql` through the extended protocol with pre-encoded params.
    /// The driver validates `expected_columns` against the server's
    /// `RowDescription`.
    ExtendedQuery {
        sql: String,
        params: Vec<Option<Vec<u8>>>,
        /// Format codes for parameters (0 = text, 1 = binary).
        param_formats: Vec<i16>,
        /// Format codes for result columns (0 = text, 1 = binary).
        result_formats: Vec<i16>,
        /// `Some(n)`: this is a Query, expect `RowDescription` with
        /// `n` columns. `None`: this is a Command, no rows expected.
        expected_columns: Option<usize>,
        reply: ExtendedReply,
    },
    /// Prepare a named statement: `Parse(named) + Describe(statement) + Sync`.
    Prepare {
        name: String,
        sql: String,
        param_oids: Vec<u32>,
        reply: PrepareReply,
    },
    /// Execute an already-prepared statement:
    /// `Bind(named) + Execute + Sync`.
    ExecutePrepared {
        stmt_name: String,
        params: Vec<Option<Vec<u8>>>,
        param_formats: Vec<i16>,
        result_formats: Vec<i16>,
        expected_columns: Option<usize>,
        reply: ExtendedReply,
    },
    /// Bind a named statement to a named portal:
    /// `Bind(portal, stmt) + Sync`.
    BindPortal {
        portal_name: String,
        stmt_name: String,
        params: Vec<Option<Vec<u8>>>,
        param_formats: Vec<i16>,
        result_formats: Vec<i16>,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Execute a portal with a row limit:
    /// `Execute(portal, max_rows) + Sync`.
    /// Returns rows and whether more rows remain.
    ExecutePortal {
        portal_name: String,
        max_rows: i32,
        reply: oneshot::Sender<Result<PortalBatch>>,
    },
    /// Close a named portal: `Close(portal) + Sync`.
    ClosePortal {
        name: String,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Close a named prepared statement: `Close(statement) + Sync`.
    CloseStatement {
        name: String,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Send `Terminate` and exit the loop. The reply fires once the socket
    /// is closed.
    Close { reply: oneshot::Sender<Result<()>> },
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SimpleQuery { sql, .. } => {
                f.debug_struct("SimpleQuery").field("sql", sql).finish()
            }
            Self::ExtendedQuery {
                sql,
                params,
                expected_columns,
                param_formats,
                ..
            } => f
                .debug_struct("ExtendedQuery")
                .field("sql", sql)
                .field("params_len", &params.len())
                .field("param_formats_len", &param_formats.len())
                .field("expected_columns", expected_columns)
                .finish(),
            Self::Prepare { name, sql, .. } => f
                .debug_struct("Prepare")
                .field("name", name)
                .field("sql", sql)
                .finish(),
            Self::ExecutePrepared {
                stmt_name,
                params,
                expected_columns,
                ..
            } => f
                .debug_struct("ExecutePrepared")
                .field("stmt_name", stmt_name)
                .field("params_len", &params.len())
                .field("expected_columns", expected_columns)
                .finish(),
            Self::BindPortal {
                portal_name,
                stmt_name,
                ..
            } => f
                .debug_struct("BindPortal")
                .field("portal_name", portal_name)
                .field("stmt_name", stmt_name)
                .finish(),
            Self::ExecutePortal {
                portal_name,
                max_rows,
                ..
            } => f
                .debug_struct("ExecutePortal")
                .field("portal_name", portal_name)
                .field("max_rows", max_rows)
                .finish(),
            Self::ClosePortal { name, .. } => {
                f.debug_struct("ClosePortal").field("name", name).finish()
            }
            Self::CloseStatement { name, .. } => f
                .debug_struct("CloseStatement")
                .field("name", name)
                .finish(),
            Self::Close { .. } => f.debug_struct("Close").finish(),
        }
    }
}

/// Snapshot of `ParameterStatus` messages observed during startup.
#[derive(Debug, Clone, Default)]
pub struct ServerParams {
    inner: Arc<HashMap<String, String>>,
}

impl ServerParams {
    pub(crate) fn from_map(map: HashMap<String, String>) -> Self {
        Self {
            inner: Arc::new(map),
        }
    }

    /// Look up a parameter (e.g. `"server_version"`).
    pub fn get(&self, name: &str) -> Option<&str> {
        self.inner.get(name).map(String::as_str)
    }

    /// Iterate all `(name, value)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inner.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BackendKeyData {
    pub process_id: i32,
    pub secret_key: i32,
}

/// Internal: spawn the driver and return everything the public `Session`
/// needs to wrap it.
pub(crate) async fn spawn(
    read: OwnedReadHalf,
    write: OwnedWriteHalf,
    params: HashMap<String, String>,
    key_data: Option<BackendKeyData>,
) -> Result<(mpsc::Sender<Command>, ServerParams, Option<BackendKeyData>)> {
    let (tx, rx) = mpsc::channel(super::COMMAND_BUFFER_SIZE);
    let driver = Driver::new(read, write, rx);
    tokio::spawn(driver.run());
    Ok((tx, ServerParams::from_map(params), key_data))
}

struct Driver {
    reader: FramedRead<OwnedReadHalf, BackendCodec>,
    writer: OwnedWriteHalf,
    inbox: mpsc::Receiver<Command>,
    /// Pending command in flight. M0 is non-pipelined: at most one.
    pending: Option<Pending>,
    write_buf: BytesMut,
}

/// State for the in-flight command.
enum Pending {
    SimpleQuery {
        reply: SimpleQueryReply,
        results: Vec<RawRows>,
        current_rows: RawRows,
        current_fields: Option<Vec<RowField>>,
        error: Option<Error>,
    },
    Extended {
        reply: ExtendedReply,
        /// `Some(n)` means a Query expecting `n` columns; `None` means
        /// a Command (no rows). Used for the `ColumnAlignment` check.
        expected_columns: Option<usize>,
        rows: RawRows,
        command_tag: Option<String>,
        error: Option<Error>,
    },
    /// Waiting for `ParseComplete + ParameterDescription + RowDescription/NoData + ReadyForQuery`.
    Prepare {
        reply: PrepareReply,
        param_oids: Option<Vec<u32>>,
        row_fields: Option<Vec<RowField>>,
        error: Option<Error>,
    },
    /// Same as Extended but for a pre-prepared statement.
    ExecutePrepared {
        reply: ExtendedReply,
        /// `Some(n)` means a prepared query expecting `n` columns; `None`
        /// means a prepared command.
        expected_columns: Option<usize>,
        rows: RawRows,
        command_tag: Option<String>,
        error: Option<Error>,
    },
    /// Waiting for `BindComplete + ReadyForQuery`.
    BindPortal {
        reply: oneshot::Sender<Result<()>>,
        error: Option<Error>,
    },
    /// Waiting for `DataRow`* + (`CommandComplete` | `PortalSuspended`) + `ReadyForQuery`.
    ExecutePortal {
        reply: oneshot::Sender<Result<PortalBatch>>,
        rows: RawRows,
        has_more: bool,
        error: Option<Error>,
    },
    /// Waiting for `CloseComplete + ReadyForQuery`.
    ClosePortal {
        reply: oneshot::Sender<Result<()>>,
        error: Option<Error>,
    },
    /// Waiting for `CloseComplete + ReadyForQuery`.
    CloseStatement {
        reply: oneshot::Sender<Result<()>>,
        error: Option<Error>,
    },
    Close {
        reply: oneshot::Sender<Result<()>>,
    },
}

impl Driver {
    fn new(read: OwnedReadHalf, write: OwnedWriteHalf, inbox: mpsc::Receiver<Command>) -> Self {
        Self {
            reader: FramedRead::new(read, BackendCodec),
            writer: write,
            inbox,
            pending: None,
            write_buf: BytesMut::with_capacity(512),
        }
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                // Always drain the socket before accepting new commands.
                msg = self.reader.next() => {
                    match msg {
                        Some(Ok(message)) => {
                            self.on_message(message);
                        }
                        Some(Err(e)) => {
                            self.fail_pending(e);
                            break;
                        }
                        None => {
                            // Socket EOF.
                            self.fail_pending(Error::Closed);
                            break;
                        }
                    }
                }
                cmd = self.inbox.recv(), if self.pending.is_none() => {
                    if let Some(c) = cmd {
                        if let Err(e) = self.start(c).await {
                            // start() already replied with the error if relevant
                            debug!("driver start failed: {e:?}");
                        }
                    } else {
                        // Session dropped — clean shutdown.
                        self.shutdown().await;
                        break;
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn start(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::SimpleQuery { sql, reply } => {
                self.write_buf.clear();
                if let Err(e) = frontend::query(&sql, &mut self.write_buf) {
                    let _ = reply.send(Err(e));
                    return Ok(());
                }
                if let Err(e) = self.flush().await {
                    let _ = reply.send(Err(e.clone_for_caller()));
                    return Err(e);
                }
                self.pending = Some(Pending::SimpleQuery {
                    reply,
                    results: Vec::new(),
                    current_rows: Vec::new(),
                    current_fields: None,
                    error: None,
                });
                Ok(())
            }
            Command::ExtendedQuery {
                sql,
                params,
                param_formats,
                result_formats,
                expected_columns,
                reply,
            } => {
                self.write_buf.clear();
                let mut build = || -> Result<()> {
                    // Unnamed prepared statement and unnamed portal.
                    frontend::parse("", &sql, std::iter::empty(), &mut self.write_buf)?;
                    frontend::bind(
                        "",
                        "",
                        &param_formats,
                        &params,
                        &result_formats,
                        &mut self.write_buf,
                    )?;
                    frontend::describe_portal("", &mut self.write_buf)?;
                    frontend::execute("", 0, &mut self.write_buf)?;
                    frontend::sync(&mut self.write_buf);
                    Ok(())
                };
                if let Err(e) = build() {
                    let _ = reply.send(Err(e));
                    return Ok(());
                }
                if let Err(e) = self.flush().await {
                    let _ = reply.send(Err(e.clone_for_caller()));
                    return Err(e);
                }
                self.pending = Some(Pending::Extended {
                    reply,
                    expected_columns,
                    rows: Vec::new(),
                    command_tag: None,
                    error: None,
                });
                Ok(())
            }
            Command::Prepare {
                name,
                sql,
                param_oids,
                reply,
            } => {
                self.write_buf.clear();
                let mut build = || -> Result<()> {
                    frontend::parse(&name, &sql, param_oids.iter().copied(), &mut self.write_buf)?;
                    frontend::describe_statement(&name, &mut self.write_buf)?;
                    frontend::sync(&mut self.write_buf);
                    Ok(())
                };
                if let Err(e) = build() {
                    let _ = reply.send(Err(e));
                    return Ok(());
                }
                if let Err(e) = self.flush().await {
                    let _ = reply.send(Err(e.clone_for_caller()));
                    return Err(e);
                }
                self.pending = Some(Pending::Prepare {
                    reply,
                    param_oids: None,
                    row_fields: None,
                    error: None,
                });
                Ok(())
            }
            Command::ExecutePrepared {
                stmt_name,
                params,
                param_formats,
                result_formats,
                expected_columns,
                reply,
            } => {
                self.write_buf.clear();
                let mut build = || -> Result<()> {
                    frontend::bind(
                        "",
                        &stmt_name,
                        &param_formats,
                        &params,
                        &result_formats,
                        &mut self.write_buf,
                    )?;
                    frontend::execute("", 0, &mut self.write_buf)?;
                    frontend::sync(&mut self.write_buf);
                    Ok(())
                };
                if let Err(e) = build() {
                    let _ = reply.send(Err(e));
                    return Ok(());
                }
                if let Err(e) = self.flush().await {
                    let _ = reply.send(Err(e.clone_for_caller()));
                    return Err(e);
                }
                self.pending = Some(Pending::ExecutePrepared {
                    reply,
                    expected_columns,
                    rows: Vec::new(),
                    command_tag: None,
                    error: None,
                });
                Ok(())
            }
            Command::BindPortal {
                portal_name,
                stmt_name,
                params,
                param_formats,
                result_formats,
                reply,
            } => {
                self.write_buf.clear();
                let mut build = || -> Result<()> {
                    frontend::bind(
                        &portal_name,
                        &stmt_name,
                        &param_formats,
                        &params,
                        &result_formats,
                        &mut self.write_buf,
                    )?;
                    frontend::sync(&mut self.write_buf);
                    Ok(())
                };
                if let Err(e) = build() {
                    let _ = reply.send(Err(e));
                    return Ok(());
                }
                if let Err(e) = self.flush().await {
                    let _ = reply.send(Err(e.clone_for_caller()));
                    return Err(e);
                }
                self.pending = Some(Pending::BindPortal { reply, error: None });
                Ok(())
            }
            Command::ExecutePortal {
                portal_name,
                max_rows,
                reply,
            } => {
                self.write_buf.clear();
                let mut build = || -> Result<()> {
                    frontend::execute(&portal_name, max_rows, &mut self.write_buf)?;
                    frontend::sync(&mut self.write_buf);
                    Ok(())
                };
                if let Err(e) = build() {
                    let _ = reply.send(Err(e));
                    return Ok(());
                }
                if let Err(e) = self.flush().await {
                    let _ = reply.send(Err(e.clone_for_caller()));
                    return Err(e);
                }
                self.pending = Some(Pending::ExecutePortal {
                    reply,
                    rows: Vec::new(),
                    has_more: false,
                    error: None,
                });
                Ok(())
            }
            Command::ClosePortal { name, reply } => {
                self.write_buf.clear();
                let mut build = || -> Result<()> {
                    frontend::close_portal(&name, &mut self.write_buf)?;
                    frontend::sync(&mut self.write_buf);
                    Ok(())
                };
                if let Err(e) = build() {
                    let _ = reply.send(Err(e));
                    return Ok(());
                }
                if let Err(e) = self.flush().await {
                    let _ = reply.send(Err(e.clone_for_caller()));
                    return Err(e);
                }
                self.pending = Some(Pending::ClosePortal { reply, error: None });
                Ok(())
            }
            Command::CloseStatement { name, reply } => {
                self.write_buf.clear();
                let mut build = || -> Result<()> {
                    frontend::close_statement(&name, &mut self.write_buf)?;
                    frontend::sync(&mut self.write_buf);
                    Ok(())
                };
                if let Err(e) = build() {
                    let _ = reply.send(Err(e));
                    return Ok(());
                }
                if let Err(e) = self.flush().await {
                    let _ = reply.send(Err(e.clone_for_caller()));
                    return Err(e);
                }
                self.pending = Some(Pending::CloseStatement { reply, error: None });
                Ok(())
            }
            Command::Close { reply } => {
                self.write_buf.clear();
                frontend::terminate(&mut self.write_buf);
                let flush_res = self.flush().await;
                let shut_res = self.writer.shutdown().await;
                let result = flush_res.and_then(|()| shut_res.map_err(Error::from));
                let _ = reply.send(result);
                // Drain inbox, fail anything pending.
                self.inbox.close();
                while let Some(extra) = self.inbox.recv().await {
                    fail_command(extra, Error::Closed);
                }
                self.pending = Some(Pending::Close {
                    reply: oneshot::channel().0,
                });
                // Mark a sentinel so the run loop exits next iteration.
                Err(Error::Closed)
            }
        }
    }

    #[allow(
        clippy::too_many_lines,
        clippy::match_same_arms,
        reason = "explicit per-state handling reads more clearly than a merged dispatch"
    )]
    fn on_message(&mut self, msg: BackendMessage) {
        trace!(?msg, "backend message");
        match (&mut self.pending, msg) {
            (
                Some(Pending::SimpleQuery { current_fields, .. }),
                BackendMessage::RowDescription { fields },
            ) => {
                *current_fields = Some(fields);
            }
            (
                Some(Pending::SimpleQuery { current_rows, .. }),
                BackendMessage::DataRow { columns },
            ) => {
                current_rows.push(columns);
            }
            (
                Some(Pending::SimpleQuery {
                    results,
                    current_rows,
                    current_fields,
                    ..
                }),
                BackendMessage::CommandComplete { .. },
            ) => {
                results.push(std::mem::take(current_rows));
                *current_fields = None;
            }
            (Some(Pending::SimpleQuery { results, .. }), BackendMessage::EmptyQueryResponse) => {
                results.push(Vec::new());
            }
            (
                Some(Pending::SimpleQuery { error, .. }),
                BackendMessage::ErrorResponse { fields },
            ) => {
                *error = Some(server_error(fields));
            }
            (Some(Pending::SimpleQuery { .. }), BackendMessage::ReadyForQuery { .. }) => {
                if let Some(Pending::SimpleQuery {
                    reply,
                    results,
                    error,
                    ..
                }) = self.pending.take()
                {
                    let outcome = error.map_or(Ok(results), Err);
                    let _ = reply.send(outcome);
                }
            }
            // ---- Extended protocol -----------------------------------
            (
                Some(Pending::Extended { .. }),
                BackendMessage::ParseComplete | BackendMessage::BindComplete,
            ) => {}
            (
                Some(Pending::Extended {
                    expected_columns,
                    error,
                    ..
                }),
                BackendMessage::RowDescription { fields },
            ) => {
                if let Some(expected) = *expected_columns {
                    if fields.len() != expected {
                        *error = error.take().or(Some(Error::ColumnAlignment {
                            expected,
                            actual: fields.len(),
                        }));
                    }
                }
            }
            (
                Some(Pending::Extended {
                    expected_columns,
                    error,
                    ..
                }),
                BackendMessage::NoData,
            ) => {
                // Server says the statement returns no rows. Mismatch
                // only if the caller declared a Query expecting > 0
                // columns.
                if let Some(expected) = *expected_columns {
                    if expected != 0 {
                        *error = error.take().or(Some(Error::ColumnAlignment {
                            expected,
                            actual: 0,
                        }));
                    }
                }
            }
            (Some(Pending::Extended { rows, .. }), BackendMessage::DataRow { columns }) => {
                rows.push(columns);
            }
            (
                Some(Pending::Extended { command_tag, .. }),
                BackendMessage::CommandComplete { tag },
            ) => {
                *command_tag = Some(tag);
            }
            (Some(Pending::Extended { .. }), BackendMessage::EmptyQueryResponse) => {
                // Treat as a successful no-op command. command_tag stays None.
            }
            (Some(Pending::Extended { error, .. }), BackendMessage::ErrorResponse { fields }) => {
                *error = error.take().or(Some(server_error(fields)));
            }
            (Some(Pending::Extended { .. }), BackendMessage::ReadyForQuery { .. }) => {
                if let Some(Pending::Extended {
                    reply,
                    rows,
                    command_tag,
                    error,
                    ..
                }) = self.pending.take()
                {
                    let outcome =
                        error.map_or_else(|| Ok(ExtendedOutcome { rows, command_tag }), Err);
                    let _ = reply.send(outcome);
                }
            }
            // ---- Prepare (named statement) ------------------------------
            (Some(Pending::Prepare { .. }), BackendMessage::ParseComplete) => {}
            (
                Some(Pending::Prepare {
                    param_oids: slot, ..
                }),
                BackendMessage::ParameterDescription { type_oids },
            ) => {
                *slot = Some(type_oids);
            }
            (
                Some(Pending::Prepare {
                    row_fields: slot, ..
                }),
                BackendMessage::RowDescription { fields },
            ) => {
                *slot = Some(fields);
            }
            (
                Some(Pending::Prepare {
                    row_fields: slot, ..
                }),
                BackendMessage::NoData,
            ) => {
                *slot = Some(Vec::new());
            }
            (Some(Pending::Prepare { error, .. }), BackendMessage::ErrorResponse { fields }) => {
                *error = error.take().or(Some(server_error(fields)));
            }
            (Some(Pending::Prepare { .. }), BackendMessage::ReadyForQuery { .. }) => {
                if let Some(Pending::Prepare {
                    reply,
                    param_oids,
                    row_fields,
                    error,
                }) = self.pending.take()
                {
                    let outcome = if let Some(e) = error {
                        Err(e)
                    } else {
                        Ok(PrepareOutcome {
                            param_oids: param_oids.unwrap_or_default(),
                            row_fields: row_fields.unwrap_or_default(),
                        })
                    };
                    let _ = reply.send(outcome);
                }
            }
            // ---- ExecutePrepared ----------------------------------------
            (Some(Pending::ExecutePrepared { .. }), BackendMessage::BindComplete) => {}
            (Some(Pending::ExecutePrepared { rows, .. }), BackendMessage::DataRow { columns }) => {
                rows.push(columns);
            }
            (
                Some(Pending::ExecutePrepared {
                    expected_columns,
                    error,
                    ..
                }),
                BackendMessage::RowDescription { fields },
            ) => {
                if let Some(expected) = *expected_columns {
                    if fields.len() != expected {
                        *error = error.take().or(Some(Error::ColumnAlignment {
                            expected,
                            actual: fields.len(),
                        }));
                    }
                }
            }
            (
                Some(Pending::ExecutePrepared {
                    expected_columns,
                    error,
                    ..
                }),
                BackendMessage::NoData,
            ) => {
                if let Some(expected) = *expected_columns {
                    if expected != 0 {
                        *error = error.take().or(Some(Error::ColumnAlignment {
                            expected,
                            actual: 0,
                        }));
                    }
                }
            }
            (
                Some(Pending::ExecutePrepared { command_tag, .. }),
                BackendMessage::CommandComplete { tag },
            ) => {
                *command_tag = Some(tag);
            }
            (Some(Pending::ExecutePrepared { .. }), BackendMessage::EmptyQueryResponse) => {}
            (
                Some(Pending::ExecutePrepared { error, .. }),
                BackendMessage::ErrorResponse { fields },
            ) => {
                *error = error.take().or(Some(server_error(fields)));
            }
            (Some(Pending::ExecutePrepared { .. }), BackendMessage::ReadyForQuery { .. }) => {
                if let Some(Pending::ExecutePrepared {
                    reply,
                    rows,
                    command_tag,
                    error,
                    ..
                }) = self.pending.take()
                {
                    let outcome =
                        error.map_or_else(|| Ok(ExtendedOutcome { rows, command_tag }), Err);
                    let _ = reply.send(outcome);
                }
            }
            // ---- BindPortal ----------------------------------------------
            (Some(Pending::BindPortal { .. }), BackendMessage::BindComplete) => {}
            (Some(Pending::BindPortal { error, .. }), BackendMessage::ErrorResponse { fields }) => {
                *error = error.take().or(Some(server_error(fields)));
            }
            (Some(Pending::BindPortal { .. }), BackendMessage::ReadyForQuery { .. }) => {
                if let Some(Pending::BindPortal { reply, error }) = self.pending.take() {
                    let outcome = error.map_or(Ok(()), Err);
                    let _ = reply.send(outcome);
                }
            }
            // ---- ExecutePortal ------------------------------------------
            (Some(Pending::ExecutePortal { rows, .. }), BackendMessage::DataRow { columns }) => {
                rows.push(columns);
            }
            (Some(Pending::ExecutePortal { .. }), BackendMessage::CommandComplete { .. }) => {
                // Portal exhausted — has_more stays false.
            }
            (Some(Pending::ExecutePortal { has_more, .. }), BackendMessage::PortalSuspended) => {
                *has_more = true;
            }
            (
                Some(Pending::ExecutePortal { error, .. }),
                BackendMessage::ErrorResponse { fields },
            ) => {
                *error = error.take().or(Some(server_error(fields)));
            }
            (Some(Pending::ExecutePortal { .. }), BackendMessage::ReadyForQuery { .. }) => {
                if let Some(Pending::ExecutePortal {
                    reply,
                    rows,
                    has_more,
                    error,
                }) = self.pending.take()
                {
                    let outcome = error.map_or_else(|| Ok(PortalBatch { rows, has_more }), Err);
                    let _ = reply.send(outcome);
                }
            }
            // ---- ClosePortal --------------------------------------------
            (Some(Pending::ClosePortal { .. }), BackendMessage::CloseComplete) => {}
            (
                Some(Pending::ClosePortal { error, .. }),
                BackendMessage::ErrorResponse { fields },
            ) => {
                *error = error.take().or(Some(server_error(fields)));
            }
            (Some(Pending::ClosePortal { .. }), BackendMessage::ReadyForQuery { .. }) => {
                if let Some(Pending::ClosePortal { reply, error }) = self.pending.take() {
                    let outcome = error.map_or(Ok(()), Err);
                    let _ = reply.send(outcome);
                }
            }
            // ---- CloseStatement -----------------------------------------
            (Some(Pending::CloseStatement { .. }), BackendMessage::CloseComplete) => {}
            (
                Some(Pending::CloseStatement { error, .. }),
                BackendMessage::ErrorResponse { fields },
            ) => {
                *error = error.take().or(Some(server_error(fields)));
            }
            (Some(Pending::CloseStatement { .. }), BackendMessage::ReadyForQuery { .. }) => {
                if let Some(Pending::CloseStatement { reply, error }) = self.pending.take() {
                    let outcome = error.map_or(Ok(()), Err);
                    let _ = reply.send(outcome);
                }
            }
            (
                _,
                BackendMessage::ParameterStatus { .. }
                | BackendMessage::NoticeResponse { .. }
                | BackendMessage::ReadyForQuery { .. },
            ) => {
                // ParameterStatus / NoticeResponse can arrive at any time;
                // ReadyForQuery with no in-flight command is treated as
                // spurious. Either way, no caller is waiting.
            }
            (state, other) => {
                let kind = pending_kind(state.as_ref().map(|p| -> &Pending { p }));
                warn!(?other, "unexpected backend message; pending={kind}");
            }
        }
    }

    async fn flush(&mut self) -> Result<()> {
        self.writer
            .write_all(&self.write_buf)
            .await
            .map_err(Error::from)?;
        self.write_buf.clear();
        Ok(())
    }

    async fn shutdown(&mut self) {
        self.write_buf.clear();
        frontend::terminate(&mut self.write_buf);
        let _ = self.writer.write_all(&self.write_buf).await;
        let _ = self.writer.shutdown().await;
    }

    fn fail_pending(&mut self, err: Error) {
        let pending = self.pending.take();
        match pending {
            Some(Pending::SimpleQuery { reply, .. }) => {
                let _ = reply.send(Err(err));
            }
            Some(Pending::Extended { reply, .. } | Pending::ExecutePrepared { reply, .. }) => {
                let _ = reply.send(Err(err));
            }
            Some(Pending::Prepare { reply, .. }) => {
                let _ = reply.send(Err(err));
            }
            Some(Pending::ExecutePortal { reply, .. }) => {
                let _ = reply.send(Err(err));
            }
            Some(
                Pending::BindPortal { reply, .. }
                | Pending::ClosePortal { reply, .. }
                | Pending::CloseStatement { reply, .. }
                | Pending::Close { reply },
            ) => {
                let _ = reply.send(Err(err));
            }
            None => {}
        }
        // Drain anything else queued.
        self.inbox.close();
        let mut leftover: VecDeque<Command> = VecDeque::new();
        while let Ok(extra) = self.inbox.try_recv() {
            leftover.push_back(extra);
        }
        for extra in leftover {
            fail_command(extra, Error::Closed);
        }
    }
}

fn fail_command(cmd: Command, err: Error) {
    match cmd {
        Command::SimpleQuery { reply, .. } => {
            let _ = reply.send(Err(err));
        }
        Command::ExtendedQuery { reply, .. } | Command::ExecutePrepared { reply, .. } => {
            let _ = reply.send(Err(err));
        }
        Command::Prepare { reply, .. } => {
            let _ = reply.send(Err(err));
        }
        Command::ExecutePortal { reply, .. } => {
            let _ = reply.send(Err(err));
        }
        Command::BindPortal { reply, .. }
        | Command::ClosePortal { reply, .. }
        | Command::CloseStatement { reply, .. }
        | Command::Close { reply } => {
            let _ = reply.send(Err(err));
        }
    }
}

fn server_error(fields: Vec<(u8, String)>) -> Error {
    let mut severity = String::new();
    let mut code = String::new();
    let mut message = String::new();
    for (k, v) in fields {
        match k {
            b'S' | b'V' if severity.is_empty() => severity = v,
            b'C' => code = v,
            b'M' => message = v,
            _ => {}
        }
    }
    Error::Server {
        severity,
        code,
        message,
    }
}

fn pending_kind(p: Option<&Pending>) -> &'static str {
    match p {
        None => "None",
        Some(Pending::SimpleQuery { .. }) => "SimpleQuery",
        Some(Pending::Extended { .. }) => "Extended",
        Some(Pending::Prepare { .. }) => "Prepare",
        Some(Pending::ExecutePrepared { .. }) => "ExecutePrepared",
        Some(Pending::BindPortal { .. }) => "BindPortal",
        Some(Pending::ExecutePortal { .. }) => "ExecutePortal",
        Some(Pending::ClosePortal { .. }) => "ClosePortal",
        Some(Pending::CloseStatement { .. }) => "CloseStatement",
        Some(Pending::Close { .. }) => "Close",
    }
}

impl Error {
    /// `Error::Io` carries a non-Clone `io::Error`. For the rare case where
    /// we need to surface the same flush-failure to both the in-flight
    /// caller and the run loop, we degrade to a `Closed` for the loop's
    /// copy. Acceptable: the run loop is exiting anyway.
    fn clone_for_caller(&self) -> Self {
        match self {
            Error::Io(_) | Error::Closed => Error::Closed,
            Error::Protocol(s) => Error::Protocol(s.clone()),
            Error::Auth(s) => Error::Auth(s.clone()),
            Error::UnsupportedAuth(s) => Error::UnsupportedAuth(s.clone()),
            Error::Server {
                code,
                severity,
                message,
            } => Error::Server {
                code: code.clone(),
                severity: severity.clone(),
                message: message.clone(),
            },
            Error::Config(s) => Error::Config(s.clone()),
            Error::Codec(s) => Error::Codec(s.clone()),
            Error::ColumnAlignment { expected, actual } => Error::ColumnAlignment {
                expected: *expected,
                actual: *actual,
            },
            Error::SchemaMismatch {
                position,
                expected_oid,
                actual_oid,
                ref column_name,
            } => Error::SchemaMismatch {
                position: *position,
                expected_oid: *expected_oid,
                actual_oid: *actual_oid,
                column_name: column_name.clone(),
            },
        }
    }
}
