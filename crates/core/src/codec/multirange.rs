//! Postgres multirange codec.

use std::marker::PhantomData;

use bytes::{Bytes, BytesMut};

use super::range::{binary_scalar_oid, decode_range_value, encode_range_value, Range};
use super::{Decoder, Encoder, FORMAT_BINARY};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// A Postgres multirange value.
#[derive(Debug, Clone, PartialEq)]
pub struct Multirange<T> {
    ranges: Vec<Range<T>>,
}

impl<T> Multirange<T> {
    /// Build a multirange from individual ranges.
    pub fn new(ranges: Vec<Range<T>>) -> Self {
        Self { ranges }
    }

    /// Borrow the contained ranges.
    pub fn ranges(&self) -> &[Range<T>] {
        &self.ranges
    }

    /// Consume the multirange and return the contained ranges.
    pub fn into_ranges(self) -> Vec<Range<T>> {
        self.ranges
    }
}

impl<T> From<Vec<Range<T>>> for Multirange<T> {
    fn from(ranges: Vec<Range<T>>) -> Self {
        Self::new(ranges)
    }
}

/// Codec returned by [`multirange()`].
#[derive(Debug, Clone, Copy)]
pub struct MultirangeCodec<C, T> {
    inner: C,
    oid: &'static [Oid],
    _marker: PhantomData<fn() -> T>,
}

/// Build a multirange codec from a binary scalar inner codec.
pub fn multirange<C, T>(codec: C) -> MultirangeCodec<C, T>
where
    C: Encoder<T> + Decoder<T>,
{
    let scalar_oid = <C as Encoder<T>>::oids(&codec)
        .first()
        .copied()
        .unwrap_or(0);
    let oid = multirange_oid_slice_for_scalar_oid(scalar_oid).unwrap_or(&[0]);
    MultirangeCodec {
        inner: codec,
        oid,
        _marker: PhantomData,
    }
}

impl<C, T> Encoder<Multirange<T>> for MultirangeCodec<C, T>
where
    C: Encoder<T> + Decoder<T>,
{
    fn encode(&self, value: &Multirange<T>, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let element_oid = binary_scalar_oid(&self.inner, "multirange")?;
        if self.oid[0] == 0 {
            return Err(Error::Codec(format!(
                "multirange: unsupported inner OID {element_oid}"
            )));
        }

        let mut buf = BytesMut::new();
        let count = i32::try_from(value.ranges().len())
            .map_err(|_| Error::Codec("multirange: too many ranges".into()))?;
        buf.extend_from_slice(&count.to_be_bytes());

        for range in value.ranges() {
            let len_idx = buf.len();
            buf.extend_from_slice(&0_i32.to_be_bytes());
            let start = buf.len();
            encode_range_value(&self.inner, range, &mut buf)?;
            let len = i32::try_from(buf.len() - start)
                .map_err(|_| Error::Codec("multirange: encoded range too large".into()))?;
            buf[len_idx..len_idx + 4].copy_from_slice(&len.to_be_bytes());
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

impl<C, T> Decoder<Multirange<T>> for MultirangeCodec<C, T>
where
    C: Encoder<T> + Decoder<T>,
{
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Multirange<T>> {
        let bytes = columns
            .first()
            .ok_or_else(|| Error::Codec("multirange: decoder needs 1 column, got 0".into()))?
            .as_deref()
            .ok_or_else(|| {
                Error::Codec("multirange: unexpected NULL; use nullable() to allow it".into())
            })?;
        let expected_oid = binary_scalar_oid(&self.inner, "multirange")?;
        if self.oid[0] == 0 {
            return Err(Error::Codec(format!(
                "multirange: unsupported inner OID {expected_oid}"
            )));
        }

        if bytes.len() < 4 {
            return Err(Error::Codec("multirange: invalid message size".into()));
        }
        let count = i32::from_be_bytes(bytes[..4].try_into().expect("fixed-size slice"));
        if count < 0 {
            return Err(Error::Codec("multirange: invalid range count".into()));
        }

        let mut offset = 4;
        let count = usize::try_from(count).expect("non-negative i32 fits in usize");
        let mut ranges = Vec::with_capacity(count);
        for _ in 0..count {
            let len = bytes
                .get(offset..offset + 4)
                .ok_or_else(|| Error::Codec("multirange: invalid message size".into()))?;
            let len = i32::from_be_bytes(len.try_into().expect("fixed-size slice"));
            if len < 0 {
                return Err(Error::Codec(
                    "multirange: invalid embedded range size".into(),
                ));
            }
            offset += 4;
            let len = usize::try_from(len).expect("non-negative i32 fits in usize");
            let range_bytes = bytes
                .get(offset..offset + len)
                .ok_or_else(|| Error::Codec("multirange: invalid message size".into()))?;
            ranges.push(decode_range_value(&self.inner, range_bytes, expected_oid)?);
            offset += len;
        }

        if offset != bytes.len() {
            return Err(Error::Codec("multirange: invalid message size".into()));
        }

        Ok(Multirange::new(ranges))
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

fn multirange_oid_slice_for_scalar_oid(element_oid: Oid) -> Option<&'static [Oid]> {
    match element_oid {
        types::INT4 => Some(&[types::INT4_MULTIRANGE]),
        types::INT8 => Some(&[types::INT8_MULTIRANGE]),
        types::NUMERIC => Some(&[types::NUM_MULTIRANGE]),
        types::DATE => Some(&[types::DATE_MULTIRANGE]),
        types::TIMESTAMP => Some(&[types::TS_MULTIRANGE]),
        types::TIMESTAMPTZ => Some(&[types::TSTZ_MULTIRANGE]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::{multirange, multirange_oid_slice_for_scalar_oid, Multirange};
    use crate::codec::{int4, Decoder, Encoder, Range, RangeBound};
    use crate::types;

    #[test]
    fn multirange_binary_roundtrip() {
        let codec = multirange(int4);
        let value = Multirange::new(vec![
            Range::NonEmpty {
                lower: RangeBound::Inclusive(1),
                upper: RangeBound::Exclusive(5),
            },
            Range::NonEmpty {
                lower: RangeBound::Inclusive(10),
                upper: RangeBound::Exclusive(20),
            },
        ]);

        let mut params = Vec::new();
        codec
            .encode(&value, &mut params)
            .expect("encode multirange");
        let bytes = params
            .pop()
            .expect("encoded slot")
            .map(Bytes::from)
            .expect("non-null multirange");
        let decoded = codec.decode(&[Some(bytes)]).expect("decode multirange");
        assert_eq!(decoded, value);
    }

    #[test]
    fn empty_multirange_roundtrip() {
        let codec = multirange(int4);
        let value = Multirange::<i32>::new(Vec::new());

        let mut params = Vec::new();
        codec
            .encode(&value, &mut params)
            .expect("encode multirange");
        let bytes = params
            .pop()
            .expect("encoded slot")
            .map(Bytes::from)
            .expect("non-null multirange");
        let decoded = codec.decode(&[Some(bytes)]).expect("decode multirange");
        assert_eq!(decoded, value);
    }

    #[test]
    fn numeric_multiranges_use_builtin_oid_mapping() {
        assert_eq!(
            multirange_oid_slice_for_scalar_oid(types::NUMERIC),
            Some(&[types::NUM_MULTIRANGE] as &'static [types::Oid])
        );
    }
}
