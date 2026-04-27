//! `hstore` codec.
//!
//! The Rust surface uses a `BTreeMap` wrapper so key iteration order is stable
//! in tests while still modeling `hstore` as an unordered mapping.

use std::collections::BTreeMap;

use bytes::Bytes;

use super::{Decoder, Encoder, FORMAT_TEXT};
use crate::error::{Error, Result};
use crate::types::{self, Oid, Type};

/// Stable Rust wrapper for PostgreSQL `hstore`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hstore(BTreeMap<String, Option<String>>);

impl Hstore {
    /// Build an empty `hstore`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a key/value pair.
    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: Option<String>,
    ) -> Option<Option<String>> {
        self.0.insert(key.into(), value)
    }

    /// Borrow the inner map.
    pub fn as_map(&self) -> &BTreeMap<String, Option<String>> {
        &self.0
    }

    /// Consume the wrapper and return the inner map.
    pub fn into_inner(self) -> BTreeMap<String, Option<String>> {
        self.0
    }

    /// Number of entries in the map.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<BTreeMap<String, Option<String>>> for Hstore {
    fn from(map: BTreeMap<String, Option<String>>) -> Self {
        Self(map)
    }
}

/// Codec for PostgreSQL `hstore`.
#[derive(Debug, Clone, Copy)]
pub struct HstoreCodec;

/// `hstore` codec value.
pub const hstore: HstoreCodec = HstoreCodec;

impl Encoder<Hstore> for HstoreCodec {
    fn encode(&self, value: &Hstore, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(encode_hstore_text(value).into_bytes()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[0]
    }

    fn types(&self) -> &'static [Type] {
        &[types::HSTORE_TYPE]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Decoder<Hstore> for HstoreCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Hstore> {
        let bytes = columns
            .first()
            .ok_or_else(|| Error::Codec("hstore: decoder needs 1 column, got 0".into()))?
            .as_deref()
            .ok_or_else(|| {
                Error::Codec("hstore: unexpected NULL; use nullable() to allow it".into())
            })?;
        let text = std::str::from_utf8(bytes)
            .map_err(|e| Error::Codec(format!("hstore: column not UTF-8: {e}")))?;
        parse_hstore_text(text).map_err(|e| Error::Codec(format!("hstore: {e}")))
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &[0]
    }

    fn types(&self) -> &'static [Type] {
        &[types::HSTORE_TYPE]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

fn encode_hstore_text(value: &Hstore) -> String {
    let mut out = String::new();
    for (index, (key, entry)) in value.0.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        push_quoted(&mut out, key);
        out.push_str("=>");
        match entry {
            Some(text) => push_quoted(&mut out, text),
            None => out.push_str("NULL"),
        }
    }
    out
}

fn parse_hstore_text(text: &str) -> std::result::Result<Hstore, String> {
    let mut cursor = 0;
    let bytes = text.as_bytes();
    let mut out = BTreeMap::new();

    skip_ws(bytes, &mut cursor);
    if cursor == bytes.len() {
        return Ok(Hstore::default());
    }

    while cursor < bytes.len() {
        let key = parse_quoted(bytes, &mut cursor)?;
        skip_ws(bytes, &mut cursor);
        expect(bytes, &mut cursor, b"=")?;
        expect(bytes, &mut cursor, b">")?;
        skip_ws(bytes, &mut cursor);
        let value = if bytes.get(cursor) == Some(&b'N') {
            expect(bytes, &mut cursor, b"NULL")?;
            None
        } else {
            Some(parse_quoted(bytes, &mut cursor)?)
        };
        out.insert(key, value);
        skip_ws(bytes, &mut cursor);
        if cursor >= bytes.len() {
            break;
        }
        expect(bytes, &mut cursor, b",")?;
        skip_ws(bytes, &mut cursor);
    }

    Ok(Hstore(out))
}

fn push_quoted(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
}

fn parse_quoted(bytes: &[u8], cursor: &mut usize) -> std::result::Result<String, String> {
    if bytes.get(*cursor) != Some(&b'"') {
        return Err("expected opening quote".into());
    }
    *cursor += 1;
    let mut out = Vec::new();
    while let Some(&byte) = bytes.get(*cursor) {
        *cursor += 1;
        match byte {
            b'"' => {
                return String::from_utf8(out)
                    .map_err(|_| "quoted hstore value was not valid UTF-8".to_string())
            }
            b'\\' => {
                let escaped = bytes
                    .get(*cursor)
                    .copied()
                    .ok_or_else(|| "unterminated escape".to_string())?;
                *cursor += 1;
                out.push(escaped);
            }
            _ => out.push(byte),
        }
    }
    Err("unterminated quoted string".into())
}

fn skip_ws(bytes: &[u8], cursor: &mut usize) {
    while matches!(bytes.get(*cursor), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        *cursor += 1;
    }
}

fn expect(bytes: &[u8], cursor: &mut usize, token: &[u8]) -> std::result::Result<(), String> {
    if bytes.get(*cursor..(*cursor + token.len())) == Some(token) {
        *cursor += token.len();
        Ok(())
    } else {
        Err(format!("expected {:?}", String::from_utf8_lossy(token)))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{encode_hstore_text, parse_hstore_text, Hstore};

    #[test]
    fn hstore_text_roundtrips_null_and_escaping() {
        let mut map = BTreeMap::new();
        map.insert("alpha".to_string(), Some("a\"b".to_string()));
        map.insert("beta".to_string(), None);
        let value = Hstore::from(map);

        let encoded = encode_hstore_text(&value);
        assert_eq!(encoded, "\"alpha\"=>\"a\\\"b\", \"beta\"=>NULL");
        assert_eq!(parse_hstore_text(&encoded).unwrap(), value);
    }

    #[test]
    fn empty_hstore_decodes_from_empty_text() {
        assert!(parse_hstore_text("").unwrap().is_empty());
    }
}
