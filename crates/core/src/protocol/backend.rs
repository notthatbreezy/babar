//! Decoded backend messages.
//!
//! Each variant carries owned data so the message can be queued onto the
//! driver task's reply channels without lifetime gymnastics. We allocate
//! per-message strings/`Bytes`; for M0 simplicity wins.
//!
//! Many fields here are decoded but only consumed in later milestones
//! (`RowField` metadata in M1, `BackendKeyData` on cancellation
//! post-v0.1). The decoder produces them today so the wire-level shape is
//! settled before the typed surface lands.

use bytes::Bytes;

/// One decoded message from the Postgres server.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum BackendMessage {
    /// `R` — authentication request. The associated [`AuthRequest`] enum
    /// captures every variant the driver currently understands.
    Authentication(AuthRequest),
    /// `S` — `ParameterStatus { name, value }`.
    ParameterStatus { name: String, value: String },
    /// `K` — `BackendKeyData` for cancellation.
    BackendKeyData { process_id: i32, secret_key: i32 },
    /// `Z` — `ReadyForQuery { transaction_status }`. The byte is one of
    /// `b'I'` (idle), `b'T'` (in transaction), `b'E'` (failed transaction).
    ReadyForQuery { transaction_status: u8 },
    /// `T` — `RowDescription` listing the columns in subsequent `DataRow`
    /// messages.
    RowDescription { fields: Vec<RowField> },
    /// `D` — `DataRow` carrying one row's column values. `None` indicates SQL
    /// `NULL`; `Some` carries the raw column bytes (text or binary, depending
    /// on the format codes set at Bind time).
    DataRow { columns: Vec<Option<Bytes>> },
    /// `G` — `CopyInResponse { overall_format, column_formats }`.
    CopyInResponse {
        /// Overall COPY format: `0 = text`, `1 = binary`.
        overall_format: u8,
        /// Per-column COPY format codes.
        column_formats: Vec<i16>,
    },
    /// `C` — `CommandComplete { tag }` (e.g. `"SELECT 1"`).
    CommandComplete { tag: String },
    /// `I` — `EmptyQueryResponse`.
    EmptyQueryResponse,
    /// `E` — `ErrorResponse { fields }`. Field values are owned strings.
    ErrorResponse { fields: Vec<(u8, String)> },
    /// `N` — `NoticeResponse { fields }`.
    NoticeResponse { fields: Vec<(u8, String)> },
    /// `1` — `ParseComplete`. Server has accepted a `Parse` message.
    ParseComplete,
    /// `2` — `BindComplete`. Server has bound a portal.
    BindComplete,
    /// `3` — `CloseComplete`. Server has freed a statement or portal.
    CloseComplete,
    /// `n` — `NoData`. Sent in place of `RowDescription` when the
    /// statement returns no rows.
    NoData,
    /// `t` — `ParameterDescription { type_oids }` reporting the OIDs the
    /// server inferred for the parsed statement's parameters.
    ParameterDescription { type_oids: Vec<u32> },
    /// `s` — `PortalSuspended`. Sent when an `Execute` exhausts its row
    /// limit before the portal is fully drained. M1 always uses unlimited
    /// `Execute` so this is unexpected; surfaces as a protocol error.
    PortalSuspended,
    /// Any other message identifier we don't yet care about. The byte is the
    /// message tag and the bytes are the remaining body (without the
    /// 4-byte length prefix). Used for `NotificationResponse`, replication
    /// COPY messages, etc.
    Other { tag: u8, body: Bytes },
}

/// Authentication request variant from an `R` message.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum AuthRequest {
    /// Authentication is complete; no further action.
    Ok,
    /// Server requests a cleartext password.
    CleartextPassword,
    /// Server requests an MD5-hashed password with the given 4-byte salt.
    Md5Password { salt: [u8; 4] },
    /// Server is offering SASL authentication mechanisms (typically
    /// `SCRAM-SHA-256` and possibly `SCRAM-SHA-256-PLUS`).
    SaslMechanisms { mechanisms: Vec<String> },
    /// Server's `AuthenticationSASLContinue` payload (the server-first
    /// message in SCRAM).
    SaslContinue { data: Bytes },
    /// Server's `AuthenticationSASLFinal` payload (the server-final
    /// message in SCRAM).
    SaslFinal { data: Bytes },
    /// Any other authentication code we don't yet handle.
    Unsupported { code: i32 },
}

/// One column metadata entry inside a `RowDescription`.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RowField {
    /// Column name as reported by the server.
    pub name: String,
    /// Table OID, or 0 if the column isn't from a table.
    pub table_oid: u32,
    /// Column attribute number, or 0.
    pub column_id: i16,
    /// Postgres type OID.
    pub type_oid: u32,
    /// Type size; negative for variable-length.
    pub type_size: i16,
    /// Type modifier; meaning depends on the type.
    pub type_modifier: i32,
    /// Format code: 0 = text, 1 = binary.
    pub format: i16,
}

/// Result of a parse attempt: more data needed or a complete message.
#[allow(dead_code)]
pub enum ParseStatus {
    /// We need at least this many more bytes before another parse is worth
    /// trying. May be `0` to signal "we don't know, just keep reading".
    Incomplete,
    /// A complete message was decoded; the source buffer was advanced past
    /// it.
    Complete(BackendMessage),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_impls_render() {
        // Sanity check that Debug doesn't panic on representative data.
        let msgs = [
            BackendMessage::Authentication(AuthRequest::Ok),
            BackendMessage::ReadyForQuery {
                transaction_status: b'I',
            },
            BackendMessage::CommandComplete {
                tag: "SELECT 1".to_string(),
            },
        ];
        for m in &msgs {
            let _ = format!("{m:?}");
        }
    }
}
