//! Error types and rich SQL-aware rendering.

use std::fmt;
use std::io;

use crate::migration::MigrationError;
use crate::query::Origin;

/// Convenience alias for `Result<T, babar::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// A driver-level error.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// I/O failure on the underlying socket.
    Io(io::Error),

    /// The server closed the connection unexpectedly or the driver task shut
    /// down before the request could be answered.
    Closed {
        /// SQL context for the in-flight operation, when available.
        sql: Option<String>,
        /// Macro callsite captured by [`crate::sql!`], when available.
        origin: Option<Origin>,
    },

    /// The server sent a message that violates the protocol (illegal
    /// transition, malformed frame, unexpected message in the current state).
    Protocol(String),

    /// Authentication failed. Distinct from a generic [`Error::Server`]
    /// because callers commonly want to special-case it.
    Auth(String),

    /// Authentication mechanism unsupported by this driver.
    UnsupportedAuth(String),

    /// `ErrorResponse` from the server.
    Server {
        /// SQLSTATE code (for example `23505`).
        code: String,
        /// Severity (`ERROR`, `FATAL`, and so on).
        severity: String,
        /// Primary message.
        message: String,
        /// Optional server-supplied detail.
        detail: Option<String>,
        /// Optional server-supplied hint.
        hint: Option<String>,
        /// Optional 1-based SQL character position.
        position: Option<usize>,
        /// SQL text associated with the failing command.
        sql: Option<String>,
        /// Macro callsite captured by [`crate::sql!`], when available.
        origin: Option<Origin>,
    },

    /// Configuration problem detected before any I/O is attempted.
    Config(String),

    /// A codec failed to encode or decode a value.
    Codec(String),

    /// A decoder's declared column count doesn't match the server's
    /// `RowDescription`.
    ColumnAlignment {
        /// Columns the decoder expects.
        expected: usize,
        /// Columns the server reported.
        actual: usize,
        /// SQL text associated with the failing command.
        sql: Option<String>,
        /// Macro callsite captured by [`crate::sql!`], when available.
        origin: Option<Origin>,
    },

    /// The server's column types don't match the decoder's expected OIDs.
    SchemaMismatch {
        /// 0-based column position of the first mismatch.
        position: usize,
        /// OID the decoder expected.
        expected_oid: u32,
        /// OID the server reported.
        actual_oid: u32,
        /// Column name from the server's `RowDescription`.
        column_name: String,
        /// SQL text associated with the failing command.
        sql: Option<String>,
        /// Macro callsite captured by [`crate::sql!`], when available.
        origin: Option<Origin>,
    },

    /// Migration parsing, planning, or configuration failed before execution.
    Migration(MigrationError),
}

impl Error {
    /// Construct an [`Error::Protocol`] from anything `Display`-able.
    pub(crate) fn protocol(msg: impl fmt::Display) -> Self {
        Self::Protocol(msg.to_string())
    }

    /// Construct a context-free closed-connection error.
    pub(crate) const fn closed() -> Self {
        Self::Closed {
            sql: None,
            origin: None,
        }
    }

    /// Attach SQL context to an error when the variant supports it.
    #[must_use]
    pub(crate) fn with_sql(mut self, sql: &str, origin: Option<Origin>) -> Self {
        match &mut self {
            Self::Closed {
                sql: slot,
                origin: slot_origin,
            }
            | Self::ColumnAlignment {
                sql: slot,
                origin: slot_origin,
                ..
            }
            | Self::SchemaMismatch {
                sql: slot,
                origin: slot_origin,
                ..
            }
            | Self::Server {
                sql: slot,
                origin: slot_origin,
                ..
            } => {
                if slot.is_none() {
                    *slot = Some(sql.to_string());
                }
                if slot_origin.is_none() {
                    *slot_origin = origin;
                }
            }
            Self::Io(_)
            | Self::Protocol(_)
            | Self::Auth(_)
            | Self::UnsupportedAuth(_)
            | Self::Config(_)
            | Self::Codec(_)
            | Self::Migration(_) => {}
        }
        self
    }

    pub(crate) fn from_server_fields(fields: Vec<(u8, String)>) -> Self {
        let mut severity = String::new();
        let mut code = String::new();
        let mut message = String::new();
        let mut detail = None;
        let mut hint = None;
        let mut position = None;

        for (key, value) in fields {
            match key {
                b'S' | b'V' if severity.is_empty() => severity = value,
                b'C' => code = value,
                b'M' => message = value,
                b'D' => detail = Some(value),
                b'H' => hint = Some(value),
                b'P' => position = value.parse::<usize>().ok(),
                _ => {}
            }
        }

        Self::Server {
            code,
            severity,
            message,
            detail,
            hint,
            position,
            sql: None,
            origin: None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<MigrationError> for Error {
    fn from(value: MigrationError) -> Self {
        Self::Migration(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Closed { sql, origin } => {
                write!(f, "connection closed")?;
                render_sql_context(f, sql.as_deref(), *origin, None)
            }
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
            Self::Auth(msg) => write!(f, "authentication failed: {msg}"),
            Self::UnsupportedAuth(name) => {
                write!(f, "unsupported authentication mechanism: {name}")
            }
            Self::Server {
                code,
                severity,
                message,
                detail,
                hint,
                position,
                sql,
                origin,
            } => {
                write!(f, "{severity} {code}: {message}")?;
                if let Some(detail) = detail {
                    write!(f, "\nDETAIL: {detail}")?;
                }
                if let Some(hint) = hint {
                    write!(f, "\nHINT: {hint}")?;
                }
                render_sql_context(f, sql.as_deref(), *origin, *position)
            }
            Self::Config(msg) => write!(f, "configuration error: {msg}"),
            Self::Codec(msg) => write!(f, "codec error: {msg}"),
            Self::Migration(err) => write!(f, "migration error: {err}"),
            Self::ColumnAlignment {
                expected,
                actual,
                sql,
                origin,
            } => {
                write!(
                    f,
                    "column alignment: decoder expects {expected} columns, server returned {actual}"
                )?;
                render_sql_context(f, sql.as_deref(), *origin, None)
            }
            Self::SchemaMismatch {
                position,
                expected_oid,
                actual_oid,
                column_name,
                sql,
                origin,
            } => {
                write!(
                    f,
                    "schema mismatch at column {position}: expected OID {expected_oid}, server has OID {actual_oid} (column \"{column_name}\")"
                )?;
                render_sql_context(f, sql.as_deref(), *origin, None)
            }
        }
    }
}

impl std::error::Error for Error {}

fn render_sql_context(
    f: &mut fmt::Formatter<'_>,
    sql: Option<&str>,
    origin: Option<Origin>,
    position: Option<usize>,
) -> fmt::Result {
    let Some(sql) = sql else {
        return Ok(());
    };

    if let Some(origin) = origin {
        write!(
            f,
            "\n--> {}:{}:{}",
            origin.file(),
            origin.line(),
            origin.column()
        )?;
    }

    let (line_no, line, column) = position
        .and_then(|offset| locate_sql(sql, offset))
        .unwrap_or((1, sql.lines().next().unwrap_or(sql), None));

    write!(f, "\nSQL {line_no:>2} | {line}")?;
    if let Some(column) = column {
        write!(f, "\n      | {}^", " ".repeat(column.saturating_sub(1)))?;
    }

    Ok(())
}

fn locate_sql(sql: &str, position: usize) -> Option<(usize, &str, Option<usize>)> {
    if position == 0 {
        return None;
    }

    let mut remaining = position;
    for (index, line) in sql.lines().enumerate() {
        let width = line.chars().count() + 1;
        if remaining <= width {
            let column = remaining.min(line.chars().count().saturating_add(1));
            return Some((index + 1, line, Some(column)));
        }
        remaining = remaining.saturating_sub(width);
    }

    let mut last = None;
    for (index, line) in sql.lines().enumerate() {
        last = Some((
            index + 1,
            line,
            Some(line.chars().count().saturating_add(1)),
        ));
    }
    last
}

#[cfg(test)]
mod tests {
    use super::Error;
    use crate::query::Origin;

    #[test]
    fn server_error_display_renders_sql_pointer() {
        let err = Error::Server {
            code: "42601".into(),
            severity: "ERROR".into(),
            message: "syntax error at or near \"FROM\"".into(),
            detail: None,
            hint: Some("check the SELECT list".into()),
            position: Some(8),
            sql: Some("SELECT FROM demo".into()),
            origin: Some(Origin::new("src/main.rs", 10, 5)),
        };

        let rendered = err.to_string();
        assert!(rendered.contains("ERROR 42601: syntax error at or near \"FROM\""));
        assert!(rendered.contains("HINT: check the SELECT list"));
        assert!(rendered.contains("--> src/main.rs:10:5"));
        assert!(rendered.contains("SQL  1 | SELECT FROM demo"));
        assert!(rendered.contains("|        ^"));
    }

    #[test]
    fn closed_error_can_render_sql_context() {
        let rendered = Error::closed()
            .with_sql("SELECT 1", Some(Origin::new("lib.rs", 2, 1)))
            .to_string();
        assert!(rendered.contains("connection closed"));
        assert!(rendered.contains("SQL  1 | SELECT 1"));
    }
}
