//! `citext` codec.
//!
//! `citext` is extension-defined, so its OID is resolved per session when a
//! statement is prepared. The Rust value surface intentionally stays `String`;
//! the distinct `citext` codec value is what selects PostgreSQL's
//! case-insensitive semantics.

use bytes::Bytes;

use super::{Decoder, Encoder, FORMAT_TEXT};
use crate::error::{Error, Result};
use crate::types::{self, Oid, Type};

/// Codec for PostgreSQL `citext`, mapped to Rust `String`.
#[derive(Debug, Clone, Copy)]
pub struct CitextCodec;

/// `citext` codec value.
pub const citext: CitextCodec = CitextCodec;

impl Encoder<String> for CitextCodec {
    fn encode(&self, value: &String, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.as_bytes().to_vec()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[0]
    }

    fn types(&self) -> &'static [Type] {
        &[types::CITEXT_TYPE]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Decoder<String> for CitextCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<String> {
        let bytes = columns
            .first()
            .ok_or_else(|| Error::Codec("citext: decoder needs 1 column, got 0".into()))?
            .as_deref()
            .ok_or_else(|| {
                Error::Codec("citext: unexpected NULL; use nullable() to allow it".into())
            })?;
        let text = std::str::from_utf8(bytes)
            .map_err(|e| Error::Codec(format!("citext: column not UTF-8: {e}")))?;
        Ok(text.to_owned())
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &[0]
    }

    fn types(&self) -> &'static [Type] {
        &[types::CITEXT_TYPE]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

#[cfg(test)]
mod tests {
    use super::citext;
    use crate::codec::{Decoder, Encoder};
    use crate::types;

    #[test]
    fn citext_reports_dynamic_type_metadata() {
        assert_eq!(Encoder::<String>::oids(&citext), &[0]);
        assert_eq!(Decoder::<String>::oids(&citext), &[0]);
        assert_eq!(Encoder::<String>::types(&citext), &[types::CITEXT_TYPE]);
        assert_eq!(Decoder::<String>::types(&citext), &[types::CITEXT_TYPE]);
    }
}
