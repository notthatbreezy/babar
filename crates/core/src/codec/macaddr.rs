//! `macaddr` / `macaddr8` codecs.

use std::fmt;
use std::str::FromStr;

use bytes::Bytes;

use super::{Decoder, Encoder, FORMAT_TEXT};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Rust value model for PostgreSQL `macaddr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MacAddr([u8; 6]);

impl MacAddr {
    /// Build a MAC-48 value from raw octets.
    pub const fn new(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }

    /// Borrow the raw octets.
    pub const fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }

    /// Consume the value and return the raw octets.
    pub const fn into_bytes(self) -> [u8; 6] {
        self.0
    }
}

impl From<[u8; 6]> for MacAddr {
    fn from(bytes: [u8; 6]) -> Self {
        Self::new(bytes)
    }
}

impl fmt::Display for MacAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_mac_octets(f, &self.0)
    }
}

impl FromStr for MacAddr {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        parse_mac::<6>(s).map(Self)
    }
}

/// Rust value model for PostgreSQL `macaddr8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MacAddr8([u8; 8]);

impl MacAddr8 {
    /// Build a MAC-64 value from raw octets.
    pub const fn new(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    /// Borrow the raw octets.
    pub const fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }

    /// Consume the value and return the raw octets.
    pub const fn into_bytes(self) -> [u8; 8] {
        self.0
    }
}

impl From<[u8; 8]> for MacAddr8 {
    fn from(bytes: [u8; 8]) -> Self {
        Self::new(bytes)
    }
}

impl fmt::Display for MacAddr8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_mac_octets(f, &self.0)
    }
}

impl FromStr for MacAddr8 {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        parse_mac::<8>(s).map(Self)
    }
}

/// Codec for `macaddr`.
#[derive(Debug, Clone, Copy)]
pub struct MacaddrCodec;

/// Codec for `macaddr8`.
#[derive(Debug, Clone, Copy)]
pub struct Macaddr8Codec;

/// `macaddr` codec value.
pub const macaddr: MacaddrCodec = MacaddrCodec;
/// `macaddr8` codec value.
pub const macaddr8: Macaddr8Codec = Macaddr8Codec;

impl Encoder<MacAddr> for MacaddrCodec {
    fn encode(&self, value: &MacAddr, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_string().into_bytes()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::MACADDR]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Decoder<MacAddr> for MacaddrCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<MacAddr> {
        decode_text::<MacAddr>(columns, "macaddr")
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::MACADDR]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Encoder<MacAddr8> for Macaddr8Codec {
    fn encode(&self, value: &MacAddr8, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_string().into_bytes()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::MACADDR8]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Decoder<MacAddr8> for Macaddr8Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<MacAddr8> {
        decode_text::<MacAddr8>(columns, "macaddr8")
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::MACADDR8]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

fn decode_text<T>(columns: &[Option<Bytes>], kind: &str) -> Result<T>
where
    T: FromStr<Err = String>,
{
    let bytes = columns
        .first()
        .ok_or_else(|| Error::Codec(format!("{kind}: decoder needs 1 column, got 0")))?
        .as_deref()
        .ok_or_else(|| {
            Error::Codec(format!(
                "{kind}: unexpected NULL; use nullable() to allow it"
            ))
        })?;
    let text = std::str::from_utf8(bytes)
        .map_err(|e| Error::Codec(format!("{kind}: column not UTF-8: {e}")))?;
    text.parse::<T>()
        .map_err(|e| Error::Codec(format!("{kind}: {e}")))
}

fn write_mac_octets(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            f.write_str(":")?;
        }
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}

fn parse_mac<const N: usize>(text: &str) -> std::result::Result<[u8; N], String> {
    let separator = if text.contains(':') {
        ':'
    } else if text.contains('-') {
        '-'
    } else {
        return Err(format!(
            "expected {N} hexadecimal octets separated by ':' or '-'"
        ));
    };
    let parts: Vec<_> = text.split(separator).collect();
    if parts.len() != N {
        return Err(format!(
            "expected {N} hexadecimal octets, got {} in {text:?}",
            parts.len()
        ));
    }
    let mut bytes = [0_u8; N];
    for (slot, part) in bytes.iter_mut().zip(parts) {
        if part.len() != 2 {
            return Err(format!(
                "invalid octet {part:?}; expected exactly two hex digits"
            ));
        }
        *slot = u8::from_str_radix(part, 16)
            .map_err(|_| format!("invalid octet {part:?}; expected hexadecimal digits"))?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{MacAddr, MacAddr8};

    #[test]
    fn macaddr_roundtrips_display_and_parse() {
        let addr = MacAddr::from([0x08, 0x00, 0x2b, 0x01, 0x02, 0x03]);
        assert_eq!(addr.to_string(), "08:00:2b:01:02:03");
        assert_eq!(MacAddr::from_str("08:00:2B:01:02:03").unwrap(), addr);
    }

    #[test]
    fn macaddr8_roundtrips_display_and_parse() {
        let addr = MacAddr8::from([0x08, 0x00, 0x2b, 0xff, 0xfe, 0x01, 0x02, 0x03]);
        assert_eq!(addr.to_string(), "08:00:2b:ff:fe:01:02:03");
        assert_eq!(MacAddr8::from_str("08-00-2b-ff-fe-01-02-03").unwrap(), addr);
    }
}
