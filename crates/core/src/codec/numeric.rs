//! `rust_decimal::Decimal` codec.

use bytes::Bytes;
use rust_decimal::Decimal;

use super::{Decoder, Encoder, FORMAT_TEXT};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Codec for Postgres `numeric` as `rust_decimal::Decimal`.
#[derive(Debug, Clone, Copy)]
pub struct NumericCodec;

/// `numeric` codec value.
pub const numeric: NumericCodec = NumericCodec;

impl Encoder<Decimal> for NumericCodec {
    fn encode(&self, value: &Decimal, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.normalize().to_string().into_bytes()));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::NUMERIC]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Decoder<Decimal> for NumericCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Decimal> {
        let bytes = columns
            .first()
            .ok_or_else(|| Error::Codec("numeric: decoder needs 1 column, got 0".into()))?
            .as_deref()
            .ok_or_else(|| {
                Error::Codec("numeric: unexpected NULL; use nullable() to allow it".into())
            })?;
        let text = std::str::from_utf8(bytes)
            .map_err(|e| Error::Codec(format!("numeric: column not UTF-8: {e}")))?;
        text.parse()
            .map_err(|e| Error::Codec(format!("numeric: {e} (got {text:?})")))
    }

    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::NUMERIC]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use proptest::prelude::*;

    use super::*;

    fn roundtrip(value: Decimal) -> Decimal {
        let mut params = Vec::new();
        numeric.encode(&value, &mut params).unwrap();
        numeric
            .decode(&[params.into_iter().next().unwrap().map(Bytes::from)])
            .unwrap()
    }

    proptest! {
        #[test]
        fn decimal_roundtrip(unscaled in any::<i64>(), scale in 0_u32..=9) {
            let value = Decimal::from_i128_with_scale(i128::from(unscaled), scale);
            prop_assert_eq!(roundtrip(value), value);
        }
    }
}
