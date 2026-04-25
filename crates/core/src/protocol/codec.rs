//! [`tokio_util::codec::Decoder`] for backend messages.
//!
//! Postgres backend messages are framed as `<tag:u8><length:u32><body:bytes>`
//! where `length` includes the 4 length bytes themselves but not the tag.
//! We parse the body into [`BackendMessage`] variants. Anything we don't
//! understand becomes [`BackendMessage::Other`] so the driver can ignore
//! it without choking the stream.
//!
//! There is one historical wrinkle: the very first message in some old
//! flows (`SSLRequest` reply, pre-startup) is a single byte without a
//! length prefix. M0 doesn't exercise that path — we connect plain TCP and
//! send `StartupMessage` first, so every message we ever decode here is
//! length-framed.

use bytes::{Buf, Bytes, BytesMut};
use tokio_util::codec::Decoder;

use super::backend::{AuthRequest, BackendMessage, RowField};
use crate::error::{Error, Result};

/// `tokio_util::codec::Decoder` implementation for backend messages.
#[derive(Debug, Default, Clone, Copy)]
pub struct BackendCodec;

const TAG_AUTH: u8 = b'R';
const TAG_PARAMETER_STATUS: u8 = b'S';
const TAG_BACKEND_KEY_DATA: u8 = b'K';
const TAG_READY_FOR_QUERY: u8 = b'Z';
const TAG_ROW_DESCRIPTION: u8 = b'T';
const TAG_DATA_ROW: u8 = b'D';
const TAG_COMMAND_COMPLETE: u8 = b'C';
const TAG_EMPTY_QUERY_RESPONSE: u8 = b'I';
const TAG_ERROR_RESPONSE: u8 = b'E';
const TAG_NOTICE_RESPONSE: u8 = b'N';

impl Decoder for BackendCodec {
    type Item = BackendMessage;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<BackendMessage>> {
        if src.len() < 5 {
            return Ok(None);
        }
        // length includes the length field itself but not the tag.
        let len = u32::from_be_bytes([src[1], src[2], src[3], src[4]]) as usize;
        if len < 4 {
            return Err(Error::protocol(format!(
                "backend message length {len} smaller than length-field width"
            )));
        }
        let total = 1 + len;
        if src.len() < total {
            // Hint to the buffer about how much we'd need before we can
            // decode again. Not strictly required.
            src.reserve(total - src.len());
            return Ok(None);
        }

        let tag = src[0];
        // Skip tag + length prefix.
        src.advance(5);
        let body_len = len - 4;
        let body = src.split_to(body_len).freeze();

        decode_body(tag, body).map(Some)
    }
}

fn decode_body(tag: u8, mut body: Bytes) -> Result<BackendMessage> {
    match tag {
        TAG_AUTH => {
            let code = read_i32(&mut body, "Authentication.code")?;
            decode_auth(code, body).map(BackendMessage::Authentication)
        }
        TAG_PARAMETER_STATUS => {
            let name = read_cstr(&mut body, "ParameterStatus.name")?;
            let value = read_cstr(&mut body, "ParameterStatus.value")?;
            Ok(BackendMessage::ParameterStatus { name, value })
        }
        TAG_BACKEND_KEY_DATA => {
            let process_id = read_i32(&mut body, "BackendKeyData.process_id")?;
            let secret_key = read_i32(&mut body, "BackendKeyData.secret_key")?;
            Ok(BackendMessage::BackendKeyData { process_id, secret_key })
        }
        TAG_READY_FOR_QUERY => {
            let status = read_u8(&mut body, "ReadyForQuery.status")?;
            Ok(BackendMessage::ReadyForQuery { transaction_status: status })
        }
        TAG_ROW_DESCRIPTION => {
            let count = read_i16(&mut body, "RowDescription.count")?;
            let count: usize = count
                .try_into()
                .map_err(|_| Error::protocol(format!("RowDescription count {count} < 0")))?;
            let mut fields = Vec::with_capacity(count);
            for _ in 0..count {
                fields.push(RowField {
                    name: read_cstr(&mut body, "RowDescription.field.name")?,
                    table_oid: read_u32(&mut body, "RowDescription.field.table_oid")?,
                    column_id: read_i16(&mut body, "RowDescription.field.column_id")?,
                    type_oid: read_u32(&mut body, "RowDescription.field.type_oid")?,
                    type_size: read_i16(&mut body, "RowDescription.field.type_size")?,
                    type_modifier: read_i32(&mut body, "RowDescription.field.type_modifier")?,
                    format: read_i16(&mut body, "RowDescription.field.format")?,
                });
            }
            Ok(BackendMessage::RowDescription { fields })
        }
        TAG_DATA_ROW => {
            let count = read_i16(&mut body, "DataRow.count")?;
            let count: usize = count
                .try_into()
                .map_err(|_| Error::protocol(format!("DataRow count {count} < 0")))?;
            let mut columns = Vec::with_capacity(count);
            for _ in 0..count {
                let len = read_i32(&mut body, "DataRow.column.length")?;
                if len == -1 {
                    columns.push(None);
                } else {
                    let len: usize = len.try_into().map_err(|_| {
                        Error::protocol(format!("DataRow column length {len} < -1"))
                    })?;
                    if body.len() < len {
                        return Err(Error::protocol(
                            "DataRow column truncated past message body",
                        ));
                    }
                    columns.push(Some(body.split_to(len)));
                }
            }
            Ok(BackendMessage::DataRow { columns })
        }
        TAG_COMMAND_COMPLETE => {
            let tag = read_cstr(&mut body, "CommandComplete.tag")?;
            Ok(BackendMessage::CommandComplete { tag })
        }
        TAG_EMPTY_QUERY_RESPONSE => Ok(BackendMessage::EmptyQueryResponse),
        TAG_ERROR_RESPONSE => Ok(BackendMessage::ErrorResponse {
            fields: read_error_fields(&mut body)?,
        }),
        TAG_NOTICE_RESPONSE => Ok(BackendMessage::NoticeResponse {
            fields: read_error_fields(&mut body)?,
        }),
        other => Ok(BackendMessage::Other { tag: other, body }),
    }
}

fn decode_auth(code: i32, body: Bytes) -> Result<AuthRequest> {
    match code {
        0 => Ok(AuthRequest::Ok),
        3 => Ok(AuthRequest::CleartextPassword),
        5 => {
            if body.len() != 4 {
                return Err(Error::protocol(format!(
                    "AuthenticationMD5Password salt length {} != 4",
                    body.len()
                )));
            }
            let mut salt = [0u8; 4];
            salt.copy_from_slice(&body[..4]);
            Ok(AuthRequest::Md5Password { salt })
        }
        10 => {
            // SASL: list of mechanism names, each NUL-terminated, list ends
            // with an extra NUL.
            let mut mechs = Vec::new();
            let mut rest = body;
            loop {
                if rest.is_empty() {
                    return Err(Error::protocol(
                        "AuthenticationSASL list missing terminator",
                    ));
                }
                if rest[0] == 0 {
                    break;
                }
                let nul = rest
                    .iter()
                    .position(|&b| b == 0)
                    .ok_or_else(|| Error::protocol("AuthenticationSASL mechanism not NUL-terminated"))?;
                let name = std::str::from_utf8(&rest[..nul])
                    .map_err(|_| Error::protocol("AuthenticationSASL mechanism is not UTF-8"))?
                    .to_string();
                mechs.push(name);
                rest.advance(nul + 1);
            }
            Ok(AuthRequest::SaslMechanisms { mechanisms: mechs })
        }
        11 => Ok(AuthRequest::SaslContinue { data: body }),
        12 => Ok(AuthRequest::SaslFinal { data: body }),
        other => Ok(AuthRequest::Unsupported { code: other }),
    }
}

fn read_error_fields(body: &mut Bytes) -> Result<Vec<(u8, String)>> {
    let mut fields = Vec::new();
    loop {
        let code = read_u8(body, "ErrorResponse.field.code")?;
        if code == 0 {
            break;
        }
        let value = read_cstr(body, "ErrorResponse.field.value")?;
        fields.push((code, value));
    }
    Ok(fields)
}

fn read_u8(buf: &mut Bytes, ctx: &'static str) -> Result<u8> {
    if buf.is_empty() {
        return Err(Error::protocol(format!("{ctx}: out of bytes")));
    }
    Ok(buf.get_u8())
}

fn read_i16(buf: &mut Bytes, ctx: &'static str) -> Result<i16> {
    if buf.len() < 2 {
        return Err(Error::protocol(format!("{ctx}: out of bytes")));
    }
    Ok(buf.get_i16())
}

fn read_i32(buf: &mut Bytes, ctx: &'static str) -> Result<i32> {
    if buf.len() < 4 {
        return Err(Error::protocol(format!("{ctx}: out of bytes")));
    }
    Ok(buf.get_i32())
}

fn read_u32(buf: &mut Bytes, ctx: &'static str) -> Result<u32> {
    if buf.len() < 4 {
        return Err(Error::protocol(format!("{ctx}: out of bytes")));
    }
    Ok(buf.get_u32())
}

fn read_cstr(buf: &mut Bytes, ctx: &'static str) -> Result<String> {
    let nul = buf
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| Error::protocol(format!("{ctx}: missing NUL terminator")))?;
    let s = std::str::from_utf8(&buf[..nul])
        .map_err(|_| Error::protocol(format!("{ctx}: not UTF-8")))?
        .to_string();
    buf.advance(nul + 1);
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BufMut;

    fn frame(tag: u8, body: &[u8]) -> BytesMut {
        let mut out = BytesMut::new();
        out.put_u8(tag);
        // length includes 4-byte length field
        let len = u32::try_from(4 + body.len()).expect("test frame fits in u32");
        out.put_u32(len);
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn decodes_ready_for_query_idle() {
        let mut buf = frame(b'Z', b"I");
        let msg = BackendCodec.decode(&mut buf).unwrap().unwrap();
        match msg {
            BackendMessage::ReadyForQuery { transaction_status } => {
                assert_eq!(transaction_status, b'I');
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(buf.is_empty());
    }

    #[test]
    fn returns_none_when_partial() {
        let mut buf = BytesMut::from(&b"Z\x00\x00"[..]);
        assert!(BackendCodec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn decodes_authentication_ok() {
        // R + length=8 + code=0 (AuthenticationOk)
        let mut buf = frame(b'R', &[0, 0, 0, 0]);
        let msg = BackendCodec.decode(&mut buf).unwrap().unwrap();
        assert!(matches!(msg, BackendMessage::Authentication(AuthRequest::Ok)));
    }

    #[test]
    fn decodes_md5_password_salt() {
        // R + length=12 + code=5 + salt(4)
        let mut buf = frame(b'R', &[0, 0, 0, 5, 0xDE, 0xAD, 0xBE, 0xEF]);
        let msg = BackendCodec.decode(&mut buf).unwrap().unwrap();
        match msg {
            BackendMessage::Authentication(AuthRequest::Md5Password { salt }) => {
                assert_eq!(salt, [0xDE, 0xAD, 0xBE, 0xEF]);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn decodes_sasl_mechanisms_list() {
        // R + length + code=10 + "SCRAM-SHA-256\0" + "\0"
        let mut payload = Vec::new();
        payload.extend_from_slice(&[0, 0, 0, 10]);
        payload.extend_from_slice(b"SCRAM-SHA-256\0");
        payload.extend_from_slice(b"\0");
        let mut buf = frame(b'R', &payload);
        let msg = BackendCodec.decode(&mut buf).unwrap().unwrap();
        match msg {
            BackendMessage::Authentication(AuthRequest::SaslMechanisms { mechanisms }) => {
                assert_eq!(mechanisms, vec!["SCRAM-SHA-256".to_string()]);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn decodes_data_row_with_null() {
        // D + length + count=2 + col0_len=1 + col0='X' + col1_len=-1 (NULL)
        let mut payload = Vec::new();
        payload.extend_from_slice(&[0, 2]);
        payload.extend_from_slice(&[0, 0, 0, 1]);
        payload.push(b'X');
        payload.extend_from_slice(&(-1_i32).to_be_bytes());
        let mut buf = frame(b'D', &payload);
        let msg = BackendCodec.decode(&mut buf).unwrap().unwrap();
        match msg {
            BackendMessage::DataRow { columns } => {
                assert_eq!(columns.len(), 2);
                assert_eq!(columns[0].as_deref(), Some(&b"X"[..]));
                assert!(columns[1].is_none());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn decodes_error_response_fields() {
        // E + length + 'S' + "ERROR\0" + 'C' + "28P01\0" + 'M' + "auth fail\0" + '\0'
        let mut payload = Vec::new();
        payload.push(b'S');
        payload.extend_from_slice(b"ERROR\0");
        payload.push(b'C');
        payload.extend_from_slice(b"28P01\0");
        payload.push(b'M');
        payload.extend_from_slice(b"auth fail\0");
        payload.push(0);
        let mut buf = frame(b'E', &payload);
        let msg = BackendCodec.decode(&mut buf).unwrap().unwrap();
        match msg {
            BackendMessage::ErrorResponse { fields } => {
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[0].0, b'S');
                assert_eq!(fields[0].1, "ERROR");
                assert_eq!(fields[1].1, "28P01");
                assert_eq!(fields[2].1, "auth fail");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
