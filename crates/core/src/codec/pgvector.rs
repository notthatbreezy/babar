//! `pgvector` codec surface.

use std::str;

use bytes::Bytes;

use super::{Decoder, Encoder, FORMAT_BINARY};
use crate::error::{Error, Result};
use crate::types::{self, Oid, Type};

const VECTOR_OID: &[Oid] = &[0];
const VECTOR_TYPE: &[Type] = &[types::VECTOR_TYPE];

/// Rust-side value for `pgvector`'s `vector` type.
///
/// The wrapper keeps the SQL type explicit and enforces the two invariants that
/// matter client-side in v1:
///
/// - vectors must not be empty
/// - every element must be finite
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    values: Vec<f32>,
}

impl Vector {
    /// Build a vector from owned `f32` values.
    pub fn new(values: Vec<f32>) -> Result<Self> {
        validate_values(&values)?;
        Ok(Self { values })
    }

    /// Borrow the vector elements.
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    /// Number of dimensions.
    pub fn dimensions(&self) -> usize {
        self.values.len()
    }

    /// Consume the wrapper and return the inner values.
    pub fn into_vec(self) -> Vec<f32> {
        self.values
    }
}

impl TryFrom<Vec<f32>> for Vector {
    type Error = Error;

    fn try_from(values: Vec<f32>) -> Result<Self> {
        Self::new(values)
    }
}

impl<const N: usize> TryFrom<[f32; N]> for Vector {
    type Error = Error;

    fn try_from(values: [f32; N]) -> Result<Self> {
        Self::new(values.into())
    }
}

/// Codec for `pgvector`'s `vector`.
#[derive(Debug, Clone, Copy)]
pub struct VectorCodec;
/// `pgvector` `vector` codec value.
pub const vector: VectorCodec = VectorCodec;

impl Encoder<Vector> for VectorCodec {
    fn encode(&self, value: &Vector, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        validate_values(value.values())?;

        let dims = i16::try_from(value.values.len())
            .map_err(|_| Error::Codec("vector: too many dimensions for binary encoding".into()))?;

        let mut buf = Vec::with_capacity(4 + value.values.len() * 4);
        buf.extend_from_slice(&dims.to_be_bytes());
        buf.extend_from_slice(&0_i16.to_be_bytes());
        for element in &value.values {
            buf.extend_from_slice(&element.to_be_bytes());
        }
        params.push(Some(buf));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        VECTOR_OID
    }

    fn types(&self) -> &'static [Type] {
        VECTOR_TYPE
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<Vector> for VectorCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Vector> {
        let bytes = vector_bytes(columns)?;
        if bytes.starts_with(b"[") {
            return parse_text_vector(bytes);
        }
        parse_binary_vector(bytes)
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        VECTOR_OID
    }

    fn types(&self) -> &'static [Type] {
        VECTOR_TYPE
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

fn vector_bytes(columns: &[Option<Bytes>]) -> Result<&[u8]> {
    let cell = columns
        .first()
        .ok_or_else(|| Error::Codec("vector: decoder needs 1 column, got 0".into()))?;
    cell.as_deref()
        .ok_or_else(|| Error::Codec("vector: unexpected NULL; use nullable() to allow it".into()))
}

fn validate_values(values: &[f32]) -> Result<()> {
    if values.is_empty() {
        return Err(Error::Codec(
            "vector: pgvector values must contain at least 1 dimension".into(),
        ));
    }
    for (index, value) in values.iter().copied().enumerate() {
        if !value.is_finite() {
            return Err(Error::Codec(format!(
                "vector: element {index} is not finite"
            )));
        }
    }
    Ok(())
}

fn parse_binary_vector(bytes: &[u8]) -> Result<Vector> {
    if bytes.len() < 4 {
        return Err(Error::Codec(format!(
            "vector: binary value too short ({} bytes)",
            bytes.len()
        )));
    }

    let dims = i16::from_be_bytes([bytes[0], bytes[1]]);
    if dims <= 0 {
        return Err(Error::Codec(format!(
            "vector: binary dimension count must be positive, got {dims}"
        )));
    }

    let unused = i16::from_be_bytes([bytes[2], bytes[3]]);
    if unused != 0 {
        return Err(Error::Codec(format!(
            "vector: expected unused header field to be 0, got {unused}"
        )));
    }

    let dims = usize::try_from(dims).expect("positive i16 fits in usize");
    let expected_len = 4 + dims * 4;
    if bytes.len() != expected_len {
        return Err(Error::Codec(format!(
            "vector: binary length mismatch, header says {dims} dims ({expected_len} bytes) but value has {} bytes",
            bytes.len()
        )));
    }

    let mut values = Vec::with_capacity(dims);
    for chunk in bytes[4..].chunks_exact(4) {
        let value = f32::from_be_bytes(chunk.try_into().expect("chunk length checked"));
        values.push(value);
    }

    Vector::new(values)
}

fn parse_text_vector(bytes: &[u8]) -> Result<Vector> {
    let text = str::from_utf8(bytes).map_err(|e| Error::Codec(format!("vector: {e}")))?;
    let text = text.trim();
    let inner = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or_else(|| Error::Codec(format!("vector: invalid text representation {text:?}")))?;

    let mut values = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(Error::Codec(format!(
                "vector: invalid text representation {text:?}"
            )));
        }
        let value = part
            .parse::<f32>()
            .map_err(|e| Error::Codec(format!("vector: {e} (got {part:?})")))?;
        values.push(value);
    }

    Vector::new(values)
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::{vector, Vector};
    use crate::codec::{Decoder, Encoder, FORMAT_BINARY};
    use crate::types;

    #[test]
    fn vector_rejects_empty_values() {
        let err = Vector::new(Vec::new()).expect_err("empty vectors should fail");
        assert!(err.to_string().contains("at least 1 dimension"));
    }

    #[test]
    fn vector_rejects_non_finite_values() {
        let err = Vector::new(vec![1.0, f32::NAN]).expect_err("nan should fail");
        assert!(err.to_string().contains("not finite"));
    }

    #[test]
    fn vector_binary_roundtrip() {
        let value = Vector::new(vec![1.0, -2.5, 3.25]).expect("vector");
        let mut params = Vec::new();
        vector.encode(&value, &mut params).expect("encode");
        let bytes = params.pop().expect("slot").expect("non-null");
        let decoded = vector.decode(&[Some(Bytes::from(bytes))]).expect("decode");
        assert_eq!(decoded, value);
    }

    #[test]
    fn vector_text_roundtrip() {
        let decoded = vector
            .decode(&[Some(Bytes::from_static(b"[1, -2.5, 3.25]"))])
            .expect("decode");
        assert_eq!(decoded.values(), &[1.0, -2.5, 3.25]);
    }

    #[test]
    fn vector_codec_reports_dynamic_type_metadata() {
        assert_eq!(Encoder::<Vector>::oids(&vector), &[0]);
        assert_eq!(Decoder::<Vector>::oids(&vector), &[0]);
        assert_eq!(Encoder::<Vector>::types(&vector), &[types::VECTOR_TYPE]);
        assert_eq!(Decoder::<Vector>::types(&vector), &[types::VECTOR_TYPE]);
        assert_eq!(Encoder::<Vector>::format_codes(&vector), &[FORMAT_BINARY]);
    }
}
