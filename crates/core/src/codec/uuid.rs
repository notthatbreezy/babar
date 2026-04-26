//! `uuid::Uuid` codec.

use bytes::{Bytes, BytesMut};
use postgres_protocol::types::{uuid_from_sql, uuid_to_sql};

use super::{Decoder, Encoder, FORMAT_BINARY};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Codec for `uuid::Uuid`.
#[derive(Debug, Clone, Copy)]
pub struct UuidCodec;

/// `uuid` codec value.
pub const uuid: UuidCodec = UuidCodec;

impl Encoder<::uuid::Uuid> for UuidCodec {
    fn encode(&self, value: &::uuid::Uuid, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let mut buf = BytesMut::with_capacity(16);
        uuid_to_sql(*value.as_bytes(), &mut buf);
        params.push(Some(buf.to_vec()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::UUID]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<::uuid::Uuid> for UuidCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<::uuid::Uuid> {
        let bytes = columns
            .first()
            .ok_or_else(|| Error::Codec("uuid: decoder needs 1 column, got 0".into()))?
            .as_deref()
            .ok_or_else(|| {
                Error::Codec("uuid: unexpected NULL; use nullable() to allow it".into())
            })?;

        if bytes.len() == 16 {
            let raw = uuid_from_sql(bytes).map_err(|e| Error::Codec(format!("uuid: {e}")))?;
            return Ok(::uuid::Uuid::from_bytes(raw));
        }

        let text = std::str::from_utf8(bytes)
            .map_err(|e| Error::Codec(format!("uuid: column not UTF-8: {e}")))?;
        text.parse()
            .map_err(|e| Error::Codec(format!("uuid: {e} (got {text:?})")))
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::UUID]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}
