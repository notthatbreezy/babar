//! Postgres array codec.

use std::marker::PhantomData;

use bytes::{Bytes, BytesMut};
use fallible_iterator::FallibleIterator;
use postgres_protocol::types::{array_from_sql, array_to_sql, ArrayDimension as PgArrayDimension};
use postgres_protocol::IsNull;

use super::{Decoder, Encoder, FORMAT_BINARY, FORMAT_TEXT};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Array dimension metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrayDimension {
    /// Length of this dimension.
    pub len: i32,
    /// Lower bound of this dimension.
    pub lower_bound: i32,
}

impl ArrayDimension {
    /// Build a dimension descriptor.
    pub const fn new(len: i32, lower_bound: i32) -> Self {
        Self { len, lower_bound }
    }
}

/// Row-major N-dimensional array value.
#[derive(Debug, Clone, PartialEq)]
pub struct Array<T> {
    dimensions: Vec<ArrayDimension>,
    values: Vec<T>,
}

impl<T> Array<T> {
    /// Create an array from explicit dimensions and row-major values.
    pub fn new(dimensions: Vec<ArrayDimension>, values: Vec<T>) -> Result<Self> {
        validate_shape(&dimensions, values.len())?;
        Ok(Self { dimensions, values })
    }

    /// Build a one-dimensional array with the default Postgres lower bound of 1.
    pub fn from_vec(values: Vec<T>) -> Self {
        let dimensions = if values.is_empty() {
            Vec::new()
        } else {
            vec![ArrayDimension::new(
                i32::try_from(values.len()).expect("1-D array length fits in i32"),
                1,
            )]
        };
        Self { dimensions, values }
    }

    /// Dimension descriptors.
    pub fn dimensions(&self) -> &[ArrayDimension] {
        &self.dimensions
    }

    /// Row-major values.
    pub fn values(&self) -> &[T] {
        &self.values
    }

    /// Consume the array and return the row-major values.
    pub fn into_values(self) -> Vec<T> {
        self.values
    }
}

/// Codec returned by [`array()`].
#[derive(Debug, Clone, Copy)]
pub struct ArrayCodec<C, T> {
    inner: C,
    oid: &'static [Oid],
    _marker: PhantomData<fn() -> T>,
}

/// Build an array codec from a binary scalar inner codec.
pub fn array<C, T>(codec: C) -> ArrayCodec<C, T>
where
    C: Encoder<T> + Decoder<T>,
{
    let scalar_oid = <C as Encoder<T>>::oids(&codec)
        .first()
        .copied()
        .unwrap_or(0);
    let oid = array_oid_slice_for_scalar_oid(scalar_oid).unwrap_or(&[0]);
    ArrayCodec {
        inner: codec,
        oid,
        _marker: PhantomData,
    }
}

impl<C, T> Encoder<Array<T>> for ArrayCodec<C, T>
where
    C: Encoder<T> + Decoder<T>,
{
    fn encode(&self, value: &Array<T>, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        validate_shape(value.dimensions(), value.values().len())?;
        let element_oid = binary_scalar_oid(&self.inner, "array")?;
        if self.oid[0] == 0 {
            return Err(Error::Codec(format!(
                "array: unsupported inner OID {element_oid}"
            )));
        }

        let mut buf = BytesMut::new();
        array_to_sql(
            value.dimensions().iter().map(|dim| PgArrayDimension {
                len: dim.len,
                lower_bound: dim.lower_bound,
            }),
            element_oid,
            value.values().iter(),
            |item, buf| Ok(encode_scalar(&self.inner, item, buf)?),
            &mut buf,
        )
        .map_err(|e| Error::Codec(format!("array: {e}")))?;
        params.push(Some(buf.to_vec()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        self.oid
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl<C, T> Decoder<Array<T>> for ArrayCodec<C, T>
where
    C: Encoder<T> + Decoder<T>,
{
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Array<T>> {
        let bytes = columns
            .first()
            .ok_or_else(|| Error::Codec("array: decoder needs 1 column, got 0".into()))?
            .as_deref()
            .ok_or_else(|| {
                Error::Codec("array: unexpected NULL; use nullable() to allow it".into())
            })?;
        let array = array_from_sql(bytes).map_err(|e| Error::Codec(format!("array: {e}")))?;
        let expected_element_oid = binary_scalar_oid(&self.inner, "array")?;
        if self.oid[0] == 0 {
            return Err(Error::Codec(format!(
                "array: unsupported inner OID {expected_element_oid}"
            )));
        }
        if array.element_type() != expected_element_oid {
            return Err(Error::Codec(format!(
                "array: server element OID {} does not match inner codec OID {}",
                array.element_type(),
                expected_element_oid,
            )));
        }

        let mut dimensions = Vec::new();
        let mut dims = array.dimensions();
        while let Some(dim) = dims
            .next()
            .map_err(|e| Error::Codec(format!("array: {e}")))?
        {
            dimensions.push(ArrayDimension::new(dim.len, dim.lower_bound));
        }

        let mut values = Vec::new();
        let mut elems = array.values();
        while let Some(elem) = elems
            .next()
            .map_err(|e| Error::Codec(format!("array: {e}")))?
        {
            let cell = elem.map(Bytes::copy_from_slice);
            values.push(self.inner.decode(&[cell])?);
        }

        Array::new(dimensions, values)
    }

    fn n_columns(&self) -> usize {
        1
    }

    fn oids(&self) -> &'static [Oid] {
        self.oid
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

fn encode_scalar<C, T>(codec: &C, value: &T, buf: &mut BytesMut) -> Result<IsNull>
where
    C: Encoder<T>,
{
    let mut params = Vec::new();
    codec.encode(value, &mut params)?;
    if params.len() != 1 {
        return Err(Error::Codec(
            "array: inner codec must produce exactly 1 parameter slot".into(),
        ));
    }
    match params.pop().expect("len checked") {
        Some(bytes) => {
            buf.extend_from_slice(&bytes);
            Ok(IsNull::No)
        }
        None => Ok(IsNull::Yes),
    }
}

fn binary_scalar_oid<C, T>(codec: &C, context: &str) -> Result<Oid>
where
    C: Encoder<T> + Decoder<T>,
{
    if <C as Encoder<T>>::oids(codec).len() != 1 || <C as Decoder<T>>::n_columns(codec) != 1 {
        return Err(Error::Codec(format!(
            "{context}: inner codec must be scalar (1 OID / 1 column)"
        )));
    }
    let format = <C as Encoder<T>>::format_codes(codec)
        .first()
        .copied()
        .unwrap_or(FORMAT_TEXT);
    if format != FORMAT_BINARY {
        return Err(Error::Codec(format!(
            "{context}: inner codec must use binary format"
        )));
    }
    Ok(<C as Encoder<T>>::oids(codec)[0])
}

fn validate_shape(dimensions: &[ArrayDimension], value_len: usize) -> Result<()> {
    if dimensions.is_empty() {
        if value_len != 0 {
            return Err(Error::Codec(
                "array: zero-dimensional arrays must not carry element values".into(),
            ));
        }
        return Ok(());
    }

    let mut expected = 1_usize;
    for dim in dimensions {
        if dim.len < 0 {
            return Err(Error::Codec(
                "array: dimension length cannot be negative".into(),
            ));
        }
        let dim_len = usize::try_from(dim.len).expect("negative lengths rejected above");
        expected = expected.checked_mul(dim_len).ok_or_else(|| {
            Error::Codec("array: too many elements for declared dimensions".into())
        })?;
    }

    if expected != value_len {
        return Err(Error::Codec(format!(
            "array: shape declares {expected} elements, got {value_len}"
        )));
    }
    Ok(())
}

fn array_oid_slice_for_scalar_oid(element_oid: Oid) -> Option<&'static [Oid]> {
    match element_oid {
        types::BOOL => Some(&[types::BOOL_ARRAY]),
        types::BYTEA => Some(&[types::BYTEA_ARRAY]),
        types::INT2 => Some(&[types::INT2_ARRAY]),
        types::INT4 => Some(&[types::INT4_ARRAY]),
        types::TEXT => Some(&[types::TEXT_ARRAY]),
        types::INT8 => Some(&[types::INT8_ARRAY]),
        types::FLOAT4 => Some(&[types::FLOAT4_ARRAY]),
        types::FLOAT8 => Some(&[types::FLOAT8_ARRAY]),
        types::VARCHAR => Some(&[types::VARCHAR_ARRAY]),
        types::BPCHAR => Some(&[types::BPCHAR_ARRAY]),
        types::INET => Some(&[types::INET_ARRAY]),
        types::CIDR => Some(&[types::CIDR_ARRAY]),
        types::DATE => Some(&[types::DATE_ARRAY]),
        types::TIME => Some(&[types::TIME_ARRAY]),
        types::TIMESTAMP => Some(&[types::TIMESTAMP_ARRAY]),
        types::TIMESTAMPTZ => Some(&[types::TIMESTAMPTZ_ARRAY]),
        types::INTERVAL => Some(&[types::INTERVAL_ARRAY]),
        types::UUID => Some(&[types::UUID_ARRAY]),
        _ => None,
    }
}
