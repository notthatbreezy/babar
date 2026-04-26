//! Frontend message helpers.
//!
//! These wrap [`postgres_protocol::message::frontend`] with the small
//! adjustments we want for the driver task: writing into the same buffer
//! the driver flushes, and capturing the right error type.
//!
//! All functions here are infallible at runtime *if* the inputs encode to
//! valid Postgres message lengths (each message body must be ≤ `i32::MAX`
//! bytes minus its length prefix). We surface that invariant as a
//! [`crate::Error::Protocol`] rather than panicking.

use bytes::BytesMut;
use postgres_protocol::message::frontend;
use postgres_protocol::IsNull;

use crate::error::{Error, Result};
use crate::types::Oid;

/// Write a `StartupMessage` with the given parameters into `buf`.
///
/// `params` is a sequence of `(name, value)` pairs. The `user` parameter is
/// always required by Postgres; callers must include it.
pub fn startup<'a>(
    params: impl IntoIterator<Item = (&'a str, &'a str)>,
    buf: &mut BytesMut,
) -> Result<()> {
    frontend::startup_message(params, buf).map_err(map_io_to_protocol)
}

/// Write a `Query` message (simple-query protocol).
pub fn query(sql: &str, buf: &mut BytesMut) -> Result<()> {
    frontend::query(sql, buf).map_err(map_io_to_protocol)
}

/// Write a `PasswordMessage` carrying `password` in cleartext.
pub fn password_message(password: &str, buf: &mut BytesMut) -> Result<()> {
    frontend::password_message(password.as_bytes(), buf).map_err(map_io_to_protocol)
}

/// Write a `SASLInitialResponse` for SCRAM-SHA-256.
pub fn sasl_initial_response(
    mechanism: &str,
    client_first: &[u8],
    buf: &mut BytesMut,
) -> Result<()> {
    frontend::sasl_initial_response(mechanism, client_first, buf).map_err(map_io_to_protocol)
}

/// Write a `SASLResponse` carrying the client's continuation message.
pub fn sasl_response(payload: &[u8], buf: &mut BytesMut) -> Result<()> {
    frontend::sasl_response(payload, buf).map_err(map_io_to_protocol)
}

/// Write a `Terminate` message; the next thing to do is close the socket.
pub fn terminate(buf: &mut BytesMut) {
    frontend::terminate(buf);
}

/// Write a `Parse` message announcing a prepared statement under
/// `stmt_name` (use `""` for unnamed). `param_oids` is the OID list the
/// driver claims for the placeholders; the server will accept `0` for
/// "let the server infer".
pub fn parse(
    stmt_name: &str,
    sql: &str,
    param_oids: impl IntoIterator<Item = Oid>,
    buf: &mut BytesMut,
) -> Result<()> {
    frontend::parse(stmt_name, sql, param_oids, buf).map_err(map_io_to_protocol)
}

/// Write a `Bind` message with explicit format codes for both parameters
/// and results.
///
/// `param_formats`: one `i16` per parameter (0 = text, 1 = binary).
/// `result_formats`: one `i16` per result column (0 = text, 1 = binary).
/// `params`: pre-encoded parameter bytes; `None` is SQL `NULL`.
pub fn bind(
    portal_name: &str,
    stmt_name: &str,
    param_formats: &[i16],
    params: &[Option<Vec<u8>>],
    result_formats: &[i16],
    buf: &mut BytesMut,
) -> Result<()> {
    frontend::bind(
        portal_name,
        stmt_name,
        param_formats.iter().copied(),
        params.iter(),
        |slot: &Option<Vec<u8>>, out: &mut BytesMut| match slot {
            Some(bytes) => {
                out.extend_from_slice(bytes);
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(IsNull::No)
            }
            None => Ok(IsNull::Yes),
        },
        result_formats.iter().copied(),
        buf,
    )
    .map_err(map_bind_error)
}

/// Write a `Bind` message using text format for all parameters and results.
/// Convenience wrapper for the simple-query upgrade path.
#[allow(dead_code)]
pub fn bind_text(
    portal_name: &str,
    stmt_name: &str,
    params: &[Option<Vec<u8>>],
    buf: &mut BytesMut,
) -> Result<()> {
    frontend::bind(
        portal_name,
        stmt_name,
        std::iter::empty::<i16>(),
        params.iter(),
        |slot: &Option<Vec<u8>>, out: &mut BytesMut| match slot {
            Some(bytes) => {
                out.extend_from_slice(bytes);
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(IsNull::No)
            }
            None => Ok(IsNull::Yes),
        },
        std::iter::empty::<i16>(),
        buf,
    )
    .map_err(map_bind_error)
}

/// `Describe` for a portal (name; `""` for unnamed). Asks the server
/// for the resulting `RowDescription`.
pub fn describe_portal(name: &str, buf: &mut BytesMut) -> Result<()> {
    frontend::describe(b'P', name, buf).map_err(map_io_to_protocol)
}

/// `Describe` for a prepared statement. Returns `ParameterDescription`
/// followed by `RowDescription` (or `NoData`).
pub fn describe_statement(name: &str, buf: &mut BytesMut) -> Result<()> {
    frontend::describe(b'S', name, buf).map_err(map_io_to_protocol)
}

/// `Close` a prepared statement. The server responds with `CloseComplete`.
pub fn close_statement(name: &str, buf: &mut BytesMut) -> Result<()> {
    frontend::close(b'S', name, buf).map_err(map_io_to_protocol)
}

/// `Close` a portal. The server responds with `CloseComplete`.
#[allow(dead_code)]
pub fn close_portal(name: &str, buf: &mut BytesMut) -> Result<()> {
    frontend::close(b'P', name, buf).map_err(map_io_to_protocol)
}

/// `Execute` a portal up to `max_rows` rows; `0` means "no limit".
pub fn execute(portal_name: &str, max_rows: i32, buf: &mut BytesMut) -> Result<()> {
    frontend::execute(portal_name, max_rows, buf).map_err(map_io_to_protocol)
}

/// `Sync` — the boundary that flushes any pending replies and unsticks
/// the protocol after an error.
pub fn sync(buf: &mut BytesMut) {
    frontend::sync(buf);
}

/// `postgres-protocol`'s frontend writers signal "message too large" via
/// `io::Error`. The error is structural, not transient, so we surface it as
/// a protocol error.
#[allow(clippy::needless_pass_by_value)] // passed to `map_err` which moves the error in
fn map_io_to_protocol(e: std::io::Error) -> Error {
    Error::Protocol(format!("frontend message encode failed: {e}"))
}

fn map_bind_error(e: postgres_protocol::message::frontend::BindError) -> Error {
    match e {
        postgres_protocol::message::frontend::BindError::Conversion(inner) => {
            Error::Codec(format!("Bind: parameter conversion failed: {inner}"))
        }
        postgres_protocol::message::frontend::BindError::Serialization(io_err) => {
            Error::Protocol(format!("Bind: serialization failed: {io_err}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden bytes for a `StartupMessage` carrying user + database. Layout:
    ///   `length(u32)` || `protocol_version(u32)` ||
    ///   `"user\0postgres\0database\0postgres\0\0"`. Protocol version 3.0
    ///   is `0x0003_0000`.
    #[test]
    fn startup_message_golden() {
        let mut buf = BytesMut::new();
        startup([("user", "postgres"), ("database", "postgres")], &mut buf).unwrap();

        let body = b"user\0postgres\0database\0postgres\0\0";
        let length = u32::try_from(4 + 4 + body.len()).expect("startup body fits in u32");
        let mut expected = Vec::new();
        expected.extend_from_slice(&length.to_be_bytes());
        expected.extend_from_slice(&0x0003_0000_u32.to_be_bytes());
        expected.extend_from_slice(body);

        assert_eq!(buf.as_ref(), expected.as_slice());
    }

    #[test]
    fn query_message_golden() {
        // Q + length(u32 = 4 + sql.len() + 1) + sql + \0
        let mut buf = BytesMut::new();
        query("SELECT 1", &mut buf).unwrap();

        let mut expected = vec![b'Q'];
        let length = u32::try_from(4 + "SELECT 1".len() + 1).expect("query length fits in u32");
        expected.extend_from_slice(&length.to_be_bytes());
        expected.extend_from_slice(b"SELECT 1\0");
        assert_eq!(buf.as_ref(), expected.as_slice());
    }

    #[test]
    fn terminate_message_golden() {
        // X + length(4)
        let mut buf = BytesMut::new();
        terminate(&mut buf);
        assert_eq!(buf.as_ref(), &[b'X', 0, 0, 0, 4]);
    }
}
