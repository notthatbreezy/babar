//! `inet` / `cidr` codecs backed by `std::net::IpAddr`.

use std::net::IpAddr;

use bytes::{Bytes, BytesMut};
use postgres_protocol::types::{inet_from_sql, inet_to_sql};

use super::{Decoder, Encoder, FORMAT_BINARY};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Codec for `inet`.
#[derive(Debug, Clone, Copy)]
pub struct InetCodec;

/// Codec for `cidr`.
#[derive(Debug, Clone, Copy)]
pub struct CidrCodec;

/// `inet` codec value.
pub const inet: InetCodec = InetCodec;
/// `cidr` codec value.
pub const cidr: CidrCodec = CidrCodec;

impl Encoder<IpAddr> for InetCodec {
    fn encode(&self, value: &IpAddr, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(encode_host(*value)));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::INET]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<IpAddr> for InetCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<IpAddr> {
        decode_host(columns, "inet")
    }

    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INET]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Encoder<IpAddr> for CidrCodec {
    fn encode(&self, value: &IpAddr, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(encode_host(*value)));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::CIDR]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<IpAddr> for CidrCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<IpAddr> {
        decode_host(columns, "cidr")
    }

    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::CIDR]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

fn encode_host(value: IpAddr) -> Vec<u8> {
    let mut buf = BytesMut::new();
    inet_to_sql(value, host_mask(value), &mut buf);
    buf.to_vec()
}

fn decode_host(columns: &[Option<Bytes>], kind: &str) -> Result<IpAddr> {
    let bytes = columns
        .first()
        .ok_or_else(|| Error::Codec(format!("{kind}: decoder needs 1 column, got 0")))?
        .as_deref()
        .ok_or_else(|| {
            Error::Codec(format!(
                "{kind}: unexpected NULL; use nullable() to allow it"
            ))
        })?;
    let parsed = inet_from_sql(bytes).map_err(|e| Error::Codec(format!("{kind}: {e}")))?;
    if parsed.netmask() != host_mask(parsed.addr()) {
        return Err(Error::Codec(format!(
            "{kind}: {addr}/{mask} is not a host address; IpAddr codecs require a full-width netmask",
            addr = parsed.addr(),
            mask = parsed.netmask(),
        )));
    }
    Ok(parsed.addr())
}

fn host_mask(addr: IpAddr) -> u8 {
    match addr {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    }
}
