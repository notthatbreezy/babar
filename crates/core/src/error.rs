//! Error types.
//!
//! In M0 the surface is intentionally narrow: I/O, protocol violations,
//! authentication failures, server-reported errors, and channel-shutdown
//! conditions. M2 will expand this to cover schema mismatches, and M6 will
//! add caret-rendered SQL display.

use std::fmt;
use std::io;

/// Convenience alias for `Result<T, babar::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// A driver-level error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// I/O failure on the underlying socket.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// The server closed the connection unexpectedly or the driver task
    /// shut down before the request could be answered.
    #[error("connection closed")]
    Closed,

    /// The server sent a message that violates the protocol (illegal
    /// transition, malformed frame, unexpected message in the current
    /// state).
    #[error("protocol error: {0}")]
    Protocol(String),

    /// Authentication failed. Distinct from a generic [`Error::Server`]
    /// because callers commonly want to special-case it.
    #[error("authentication failed: {0}")]
    Auth(String),

    /// Authentication mechanism unsupported by this driver.
    #[error("unsupported authentication mechanism: {0}")]
    UnsupportedAuth(String),

    /// `ErrorResponse` from the server. Carries the SQLSTATE and severity at
    /// minimum; richer fields land in M6.
    #[error("server error: {severity} {code}: {message}")]
    Server {
        /// SQLSTATE code (e.g. "28P01" for invalid password).
        code: String,
        /// Severity (`ERROR`, `FATAL`, etc).
        severity: String,
        /// Primary message.
        message: String,
    },

    /// Configuration problem detected before any I/O is attempted.
    #[error("configuration error: {0}")]
    Config(String),

    /// A codec failed to encode or decode a value.
    #[error("codec error: {0}")]
    Codec(String),

    /// A decoder's declared column count doesn't match the server's
    /// `RowDescription`. The decoder shape was settled at compile time;
    /// the schema mismatch shows up at execute time in M1. (M2 catches
    /// this earlier, at prepare time.)
    #[error("column alignment: decoder expects {expected} columns, server returned {actual}")]
    ColumnAlignment {
        /// Columns the decoder expects (sum of `n_columns()` across the
        /// decoder tree).
        expected: usize,
        /// Columns the server reported in `RowDescription`.
        actual: usize,
    },
}

impl Error {
    /// Construct a [`Error::Protocol`] from anything `Display`-able.
    pub(crate) fn protocol(msg: impl fmt::Display) -> Self {
        Self::Protocol(msg.to_string())
    }
}
