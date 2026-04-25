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
/// Pub re-exported as [`crate::SimpleQueryRows`] for callers; the typed
/// [`Session::execute`]/`Session::stream` surface in M1 will replace this
/// raw shape.
///
/// [`Session::execute`]: crate::Session
pub type RawRows = Vec<Vec<Option<Bytes>>>;

/// One simple-query string can carry multiple statements; each produces a
/// `RawRows`.
type SimpleQueryReply = oneshot::Sender<Result<Vec<RawRows>>>;

/// One unit of work the driver task accepts.
#[derive(Debug)]
pub enum Command {
    /// Run `sql` through the simple-query protocol. Reply with the rows of
    /// every result set in order. `None` means SQL NULL.
    SimpleQuery {
        sql: String,
        reply: SimpleQueryReply,
    },
    /// Send `Terminate` and exit the loop. The reply fires once the socket
    /// is closed.
    Close {
        reply: oneshot::Sender<Result<()>>,
    },
}

/// Snapshot of `ParameterStatus` messages observed during startup.
#[derive(Debug, Clone, Default)]
pub struct ServerParams {
    inner: Arc<HashMap<String, String>>,
}

impl ServerParams {
    pub(crate) fn from_map(map: HashMap<String, String>) -> Self {
        Self { inner: Arc::new(map) }
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
                self.pending = Some(Pending::Close { reply: oneshot::channel().0 });
                // Mark a sentinel so the run loop exits next iteration.
                Err(Error::Closed)
            }
        }
    }

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
            (Some(Pending::SimpleQuery { error, .. }), BackendMessage::ErrorResponse { fields }) => {
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
            Some(Pending::Close { reply }) => {
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
        Command::Close { reply } => {
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
    Error::Server { severity, code, message }
}

fn pending_kind(p: Option<&Pending>) -> &'static str {
    match p {
        None => "None",
        Some(Pending::SimpleQuery { .. }) => "SimpleQuery",
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
            Error::Server { code, severity, message } => Error::Server {
                code: code.clone(),
                severity: severity.clone(),
                message: message.clone(),
            },
            Error::Config(s) => Error::Config(s.clone()),
        }
    }
}
