//! Postgres `interval` codec.

use bytes::{BufMut, Bytes};

use super::{Decoder, Encoder, FORMAT_BINARY};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Postgres interval with the wire-level month/day/microsecond split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    /// Whole months component.
    pub months: i32,
    /// Whole days component.
    pub days: i32,
    /// Time component in microseconds.
    pub microseconds: i64,
}

impl Interval {
    /// Build a new interval value.
    pub const fn new(months: i32, days: i32, microseconds: i64) -> Self {
        Self {
            months,
            days,
            microseconds,
        }
    }
}

/// Codec for [`Interval`].
#[derive(Debug, Clone, Copy)]
pub struct IntervalCodec;

/// `interval` codec value.
pub const interval: IntervalCodec = IntervalCodec;

impl Encoder<Interval> for IntervalCodec {
    fn encode(&self, value: &Interval, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let mut buf = Vec::with_capacity(16);
        buf.put_i64(value.microseconds);
        buf.put_i32(value.days);
        buf.put_i32(value.months);
        params.push(Some(buf));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::INTERVAL]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<Interval> for IntervalCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Interval> {
        let bytes = columns
            .first()
            .ok_or_else(|| Error::Codec("interval: decoder needs 1 column, got 0".into()))?
            .as_deref()
            .ok_or_else(|| {
                Error::Codec("interval: unexpected NULL; use nullable() to allow it".into())
            })?;
        if bytes.len() != 16 {
            return Err(Error::Codec(format!(
                "interval: expected 16 binary bytes, got {}",
                bytes.len()
            )));
        }
        let micros = i64::from_be_bytes(bytes[0..8].try_into().expect("len checked"));
        let days = i32::from_be_bytes(bytes[8..12].try_into().expect("len checked"));
        let months = i32::from_be_bytes(bytes[12..16].try_into().expect("len checked"));
        Ok(Interval::new(months, days, micros))
    }

    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INTERVAL]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}
