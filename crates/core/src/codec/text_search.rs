//! Text-search codec surface.

use std::fmt;
use std::str;

use bytes::Bytes;

use super::{Decoder, Encoder};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Wrapper for PostgreSQL `tsvector`.
///
/// v0.1 keeps the Rust surface intentionally narrow: the value stores the
/// canonical text-search syntax and lets PostgreSQL own parsing and
/// normalization semantics.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TsVector(String);

impl TsVector {
    /// Build a `tsvector` wrapper from canonical text-search syntax.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying `tsvector` text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the wrapper and return the inner string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl From<String> for TsVector {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for TsVector {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for TsVector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Wrapper for PostgreSQL `tsquery`.
///
/// Like [`TsVector`], this stores the canonical SQL text representation in v0.1.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TsQuery(String);

impl TsQuery {
    /// Build a `tsquery` wrapper from canonical text-search syntax.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying `tsquery` text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the wrapper and return the inner string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl From<String> for TsQuery {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for TsQuery {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for TsQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Codec for `tsvector`.
#[derive(Debug, Clone, Copy)]
pub struct TsVectorCodec;
/// `tsvector` codec value.
pub const tsvector: TsVectorCodec = TsVectorCodec;

impl Encoder<TsVector> for TsVectorCodec {
    fn encode(&self, value: &TsVector, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.as_str().as_bytes().to_vec()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::TSVECTOR]
    }
}

impl Decoder<TsVector> for TsVectorCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<TsVector> {
        Ok(TsVector::new(text_search_str(columns, "tsvector")?))
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::TSVECTOR]
    }
}

/// Codec for `tsquery`.
#[derive(Debug, Clone, Copy)]
pub struct TsQueryCodec;
/// `tsquery` codec value.
pub const tsquery: TsQueryCodec = TsQueryCodec;

impl Encoder<TsQuery> for TsQueryCodec {
    fn encode(&self, value: &TsQuery, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.as_str().as_bytes().to_vec()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::TSQUERY]
    }
}

impl Decoder<TsQuery> for TsQueryCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<TsQuery> {
        Ok(TsQuery::new(text_search_str(columns, "tsquery")?))
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::TSQUERY]
    }
}

fn text_search_str(columns: &[Option<Bytes>], type_name: &'static str) -> Result<String> {
    let cell = columns
        .first()
        .ok_or_else(|| Error::Codec(format!("{type_name}: decoder needs 1 column, got 0")))?;
    let bytes = cell.as_deref().ok_or_else(|| {
        Error::Codec(format!(
            "{type_name}: unexpected NULL; use nullable() to allow it"
        ))
    })?;
    str::from_utf8(bytes)
        .map(ToString::to_string)
        .map_err(|e| Error::Codec(format!("{type_name}: column not UTF-8: {e}")))
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::{tsquery, tsvector, TsQuery, TsVector};
    use crate::codec::{Decoder, Encoder};
    use crate::types;

    #[test]
    fn tsvector_roundtrip() {
        let value = TsVector::from("'fat':1 'rat':2");
        let mut params = Vec::new();
        tsvector.encode(&value, &mut params).expect("encode");
        let bytes = params.pop().expect("slot").expect("non-null");
        let decoded = tsvector
            .decode(&[Some(Bytes::from(bytes))])
            .expect("decode");
        assert_eq!(decoded, value);
    }

    #[test]
    fn tsquery_roundtrip() {
        let value = TsQuery::from("'fat' & 'rat'");
        let mut params = Vec::new();
        tsquery.encode(&value, &mut params).expect("encode");
        let bytes = params.pop().expect("slot").expect("non-null");
        let decoded = tsquery.decode(&[Some(Bytes::from(bytes))]).expect("decode");
        assert_eq!(decoded, value);
    }

    #[test]
    fn text_search_codecs_report_builtin_oids() {
        assert_eq!(Encoder::<TsVector>::oids(&tsvector), &[types::TSVECTOR]);
        assert_eq!(Decoder::<TsVector>::oids(&tsvector), &[types::TSVECTOR]);
        assert_eq!(Encoder::<TsQuery>::oids(&tsquery), &[types::TSQUERY]);
        assert_eq!(Decoder::<TsQuery>::oids(&tsquery), &[types::TSQUERY]);
    }
}
