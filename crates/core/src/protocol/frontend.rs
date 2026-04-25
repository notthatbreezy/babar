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

use crate::error::{Error, Result};

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
pub fn sasl_initial_response(mechanism: &str, client_first: &[u8], buf: &mut BytesMut) -> Result<()> {
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

/// `postgres-protocol`'s frontend writers signal "message too large" via
/// `io::Error`. The error is structural, not transient, so we surface it as
/// a protocol error.
#[allow(clippy::needless_pass_by_value)] // passed to `map_err` which moves the error in
fn map_io_to_protocol(e: std::io::Error) -> Error {
    Error::Protocol(format!("frontend message encode failed: {e}"))
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
