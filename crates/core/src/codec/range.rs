//! Postgres range codec.

use std::marker::PhantomData;

use bytes::{Bytes, BytesMut};
use postgres_protocol::types::{
    range_from_sql, range_to_sql, Range as PgRange, RangeBound as PgRangeBound,
};
use postgres_protocol::IsNull;

use super::{Decoder, Encoder, FORMAT_BINARY, FORMAT_TEXT};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// One side of a Postgres range.
#[derive(Debug, Clone, PartialEq)]
pub enum RangeBound<T> {
    /// Inclusive bound.
    Inclusive(T),
    /// Exclusive bound.
    Exclusive(T),
    /// No bound.
    Unbounded,
}

/// A Postgres range value.
#[derive(Debug, Clone, PartialEq)]
pub enum Range<T> {
    /// Empty range.
    Empty,
    /// Non-empty range.
    NonEmpty {
        /// Lower bound.
        lower: RangeBound<T>,
        /// Upper bound.
        upper: RangeBound<T>,
    },
}

/// Codec returned by [`range()`].
#[derive(Debug, Clone, Copy)]
pub struct RangeCodec<C, T> {
    inner: C,
    oid: &'static [Oid],
    _marker: PhantomData<fn() -> T>,
}

/// Build a range codec from a scalar inner codec.
pub fn range<C, T>(codec: C) -> RangeCodec<C, T>
where
    C: Encoder<T> + Decoder<T>,
{
    let scalar_oid = <C as Encoder<T>>::oids(&codec)
        .first()
        .copied()
        .unwrap_or(0);
    let oid = range_oid_slice_for_scalar_oid(scalar_oid).unwrap_or(&[0]);
    RangeCodec {
        inner: codec,
        oid,
        _marker: PhantomData,
    }
}

impl<C, T> Encoder<Range<T>> for RangeCodec<C, T>
where
    C: Encoder<T> + Decoder<T>,
{
    fn encode(&self, value: &Range<T>, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let element_oid = binary_scalar_oid(&self.inner, "range")?;
        if self.oid[0] == 0 {
            return Err(Error::Codec(format!(
                "range: unsupported inner OID {element_oid}"
            )));
        }

        let mut buf = BytesMut::new();
        match value {
            Range::Empty => postgres_protocol::types::empty_range_to_sql(&mut buf),
            Range::NonEmpty { lower, upper } => {
                range_to_sql(
                    |buf| Ok(encode_bound(&self.inner, lower, buf)?),
                    |buf| Ok(encode_bound(&self.inner, upper, buf)?),
                    &mut buf,
                )
                .map_err(|e| Error::Codec(format!("range: {e}")))?;
            }
        }
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

impl<C, T> Decoder<Range<T>> for RangeCodec<C, T>
where
    C: Encoder<T> + Decoder<T>,
{
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Range<T>> {
        let bytes = columns
            .first()
            .ok_or_else(|| Error::Codec("range: decoder needs 1 column, got 0".into()))?
            .as_deref()
            .ok_or_else(|| {
                Error::Codec("range: unexpected NULL; use nullable() to allow it".into())
            })?;
        let expected_oid = binary_scalar_oid(&self.inner, "range")?;
        if self.oid[0] == 0 {
            return Err(Error::Codec(format!(
                "range: unsupported inner OID {expected_oid}"
            )));
        }
        let pg_range = range_from_sql(bytes).map_err(|e| Error::Codec(format!("range: {e}")))?;
        match pg_range {
            PgRange::Empty => Ok(Range::Empty),
            PgRange::Nonempty(lower, upper) => Ok(Range::NonEmpty {
                lower: decode_bound(&self.inner, &lower, expected_oid)?,
                upper: decode_bound(&self.inner, &upper, expected_oid)?,
            }),
        }
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

fn encode_bound<C, T>(
    codec: &C,
    bound: &RangeBound<T>,
    buf: &mut BytesMut,
) -> Result<PgRangeBound<IsNull>>
where
    C: Encoder<T>,
{
    let result = match bound {
        RangeBound::Unbounded => PgRangeBound::Unbounded,
        RangeBound::Inclusive(value) => PgRangeBound::Inclusive(encode_scalar(codec, value, buf)?),
        RangeBound::Exclusive(value) => PgRangeBound::Exclusive(encode_scalar(codec, value, buf)?),
    };
    Ok(result)
}

fn decode_bound<C, T>(
    codec: &C,
    bound: &PgRangeBound<Option<&[u8]>>,
    expected_oid: Oid,
) -> Result<RangeBound<T>>
where
    C: Decoder<T>,
{
    Ok(match bound {
        PgRangeBound::Unbounded => RangeBound::Unbounded,
        PgRangeBound::Inclusive(Some(bytes)) => {
            RangeBound::Inclusive(decode_scalar(codec, bytes, expected_oid)?)
        }
        PgRangeBound::Exclusive(Some(bytes)) => {
            RangeBound::Exclusive(decode_scalar(codec, bytes, expected_oid)?)
        }
        PgRangeBound::Inclusive(None) | PgRangeBound::Exclusive(None) => {
            return Err(Error::Codec(
                "range: NULL bounds are invalid; use unbounded instead".into(),
            ))
        }
    })
}

fn encode_scalar<C, T>(codec: &C, value: &T, buf: &mut BytesMut) -> Result<IsNull>
where
    C: Encoder<T>,
{
    let mut params = Vec::new();
    codec.encode(value, &mut params)?;
    if params.len() != 1 {
        return Err(Error::Codec(
            "range: inner codec must produce exactly 1 parameter slot".into(),
        ));
    }
    match params.pop().expect("len checked") {
        Some(bytes) => {
            buf.extend_from_slice(&bytes);
            Ok(IsNull::No)
        }
        None => Err(Error::Codec(
            "range: NULL bounds are invalid; use unbounded instead".into(),
        )),
    }
}

fn decode_scalar<C, T>(codec: &C, bytes: &[u8], expected_oid: Oid) -> Result<T>
where
    C: Decoder<T>,
{
    let oid = *<C as Decoder<T>>::oids(codec)
        .first()
        .ok_or_else(|| Error::Codec("range: inner codec produced no OIDs".into()))?;
    if oid != expected_oid {
        return Err(Error::Codec(format!(
            "range: server element OID {expected_oid} does not match inner codec OID {oid}"
        )));
    }
    codec.decode(&[Some(Bytes::copy_from_slice(bytes))])
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

fn range_oid_slice_for_scalar_oid(element_oid: Oid) -> Option<&'static [Oid]> {
    match element_oid {
        types::INT4 => Some(&[types::INT4_RANGE]),
        types::INT8 => Some(&[types::INT8_RANGE]),
        types::DATE => Some(&[types::DATE_RANGE]),
        types::TIMESTAMP => Some(&[types::TS_RANGE]),
        types::TIMESTAMPTZ => Some(&[types::TSTZ_RANGE]),
        _ => None,
    }
}
