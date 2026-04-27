//! `bit` / `varbit` codecs.
//!
//! The Rust value model keeps both the packed bytes and the logical bit length
//! explicit so callers do not accidentally lose trailing zero bits.

use std::fmt;
use std::str::FromStr;

use bytes::Bytes;

use super::{Decoder, Encoder, FORMAT_TEXT};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Packed bit-string value for PostgreSQL `bit` / `varbit`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BitString {
    bytes: Vec<u8>,
    bit_len: usize,
}

impl BitString {
    /// Build a bit string from packed bytes plus an explicit logical bit length.
    pub fn from_bytes(bytes: Vec<u8>, bit_len: usize) -> Result<Self> {
        if bit_len > bytes.len().saturating_mul(8) {
            return Err(Error::Codec(format!(
                "bit string: declared length {bit_len} exceeds backing storage of {} bits",
                bytes.len() * 8
            )));
        }
        let trailing = bit_len % 8;
        if trailing != 0 {
            let mask = (1_u8 << (8 - trailing)) - 1;
            let last = bytes
                .last()
                .copied()
                .ok_or_else(|| Error::Codec("bit string: missing final byte".into()))?;
            if last & mask != 0 {
                return Err(Error::Codec(
                    "bit string: unused trailing bits must be zero".into(),
                ));
            }
        }
        Ok(Self { bytes, bit_len })
    }

    /// Parse a bit string from canonical `0101...` text.
    pub fn from_text(text: &str) -> Result<Self> {
        let mut bytes = vec![0_u8; text.len().div_ceil(8)];
        for (index, ch) in text.bytes().enumerate() {
            match ch {
                b'0' => {}
                b'1' => {
                    let byte = index / 8;
                    let bit_offset = 7 - (index % 8);
                    bytes[byte] |= 1 << bit_offset;
                }
                _ => {
                    return Err(Error::Codec(format!(
                        "bit string: invalid character {:?}; expected only '0' or '1'",
                        ch as char
                    )))
                }
            }
        }
        Self::from_bytes(bytes, text.len())
    }

    /// Number of logical bits in the value.
    pub const fn len(&self) -> usize {
        self.bit_len
    }

    /// Whether the value has zero bits.
    pub const fn is_empty(&self) -> bool {
        self.bit_len == 0
    }

    /// Borrow the packed bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Read one logical bit by index.
    pub fn get(&self, index: usize) -> Option<bool> {
        if index >= self.bit_len {
            return None;
        }
        let byte = self.bytes[index / 8];
        let mask = 1 << (7 - (index % 8));
        Some(byte & mask != 0)
    }

    /// Render the bit string as canonical `0101...` text.
    pub fn to_bit_text(&self) -> String {
        let mut out = String::with_capacity(self.bit_len);
        for index in 0..self.bit_len {
            out.push(if self.get(index).unwrap_or(false) {
                '1'
            } else {
                '0'
            });
        }
        out
    }
}

impl fmt::Display for BitString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_bit_text())
    }
}

impl FromStr for BitString {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::from_text(s).map_err(|e| e.to_string())
    }
}

/// Codec for `bit`.
#[derive(Debug, Clone, Copy)]
pub struct BitCodec;

/// Codec for `varbit`.
#[derive(Debug, Clone, Copy)]
pub struct VarbitCodec;

/// `bit` codec value.
pub const bit: BitCodec = BitCodec;
/// `varbit` codec value.
pub const varbit: VarbitCodec = VarbitCodec;

impl Encoder<BitString> for BitCodec {
    fn encode(&self, value: &BitString, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_string().into_bytes()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::BIT]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Decoder<BitString> for BitCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<BitString> {
        decode_text(columns, "bit")
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::BIT]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Encoder<BitString> for VarbitCodec {
    fn encode(&self, value: &BitString, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_string().into_bytes()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::VARBIT]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Decoder<BitString> for VarbitCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<BitString> {
        decode_text(columns, "varbit")
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::VARBIT]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

fn decode_text(columns: &[Option<Bytes>], kind: &str) -> Result<BitString> {
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
    BitString::from_text(text).map_err(|e| Error::Codec(format!("{kind}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::BitString;

    #[test]
    fn bit_string_preserves_trailing_zero_bits() {
        let value = BitString::from_text("10100000").unwrap();
        assert_eq!(value.as_bytes(), &[0b1010_0000]);
        assert_eq!(value.to_bit_text(), "10100000");
    }

    #[test]
    fn bit_string_rejects_non_zero_unused_trailing_bits() {
        let err = BitString::from_bytes(vec![0b1010_0001], 3).unwrap_err();
        assert!(err.to_string().contains("unused trailing bits"));
    }
}
