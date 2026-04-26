//! `chrono` crate codecs.

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use postgres_protocol::types::{
    date_from_sql, date_to_sql, time_from_sql, time_to_sql, timestamp_from_sql, timestamp_to_sql,
};

use super::{Decoder, Encoder, FORMAT_BINARY};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Codec for `chrono::NaiveDate`.
#[derive(Debug, Clone, Copy)]
pub struct ChronoDateCodec;
/// Codec for `chrono::NaiveTime`.
#[derive(Debug, Clone, Copy)]
pub struct ChronoTimeCodec;
/// Codec for `chrono::NaiveDateTime`.
#[derive(Debug, Clone, Copy)]
pub struct ChronoTimestampCodec;
/// Codec for `chrono::DateTime<Utc>`.
#[derive(Debug, Clone, Copy)]
pub struct ChronoDateTimeCodec;

/// `chrono` date codec value.
pub const chrono_date: ChronoDateCodec = ChronoDateCodec;
/// `chrono` time codec value.
pub const chrono_time: ChronoTimeCodec = ChronoTimeCodec;
/// `chrono` timestamp codec value.
pub const chrono_timestamp: ChronoTimestampCodec = ChronoTimestampCodec;
/// `chrono` timestamptz codec value.
pub const chrono_timestamptz: ChronoDateTimeCodec = ChronoDateTimeCodec;

impl Encoder<NaiveDate> for ChronoDateCodec {
    fn encode(&self, value: &NaiveDate, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let days = value.signed_duration_since(pg_epoch_date()).num_days();
        let days = i32::try_from(days)
            .map_err(|_| Error::Codec("chrono::NaiveDate out of Postgres range".into()))?;
        let mut buf = BytesMut::with_capacity(4);
        date_to_sql(days, &mut buf);
        params.push(Some(buf.to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::DATE]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<NaiveDate> for ChronoDateCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<NaiveDate> {
        let bytes = cell(columns, "chrono::NaiveDate")?;
        let days =
            date_from_sql(bytes).map_err(|e| Error::Codec(format!("chrono::NaiveDate: {e}")))?;
        Ok(pg_epoch_date() + Duration::days(i64::from(days)))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::DATE]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Encoder<NaiveTime> for ChronoTimeCodec {
    fn encode(&self, value: &NaiveTime, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let micros = i64::from(value.num_seconds_from_midnight()) * 1_000_000
            + i64::from(value.nanosecond() / 1_000);
        let mut buf = BytesMut::with_capacity(8);
        time_to_sql(micros, &mut buf);
        params.push(Some(buf.to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::TIME]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<NaiveTime> for ChronoTimeCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<NaiveTime> {
        let bytes = cell(columns, "chrono::NaiveTime")?;
        let micros =
            time_from_sql(bytes).map_err(|e| Error::Codec(format!("chrono::NaiveTime: {e}")))?;
        let secs = micros.div_euclid(1_000_000);
        let micros_part = micros.rem_euclid(1_000_000);
        let nanos = u32::try_from(micros_part * 1_000)
            .map_err(|_| Error::Codec("chrono::NaiveTime: nanoseconds out of range".into()))?;
        NaiveTime::from_num_seconds_from_midnight_opt(
            u32::try_from(secs)
                .map_err(|_| Error::Codec("chrono::NaiveTime: seconds out of range".into()))?,
            nanos,
        )
        .ok_or_else(|| Error::Codec("chrono::NaiveTime: invalid time value".into()))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::TIME]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Encoder<NaiveDateTime> for ChronoTimestampCodec {
    fn encode(&self, value: &NaiveDateTime, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let micros = value
            .signed_duration_since(pg_epoch())
            .num_microseconds()
            .ok_or_else(|| Error::Codec("chrono::NaiveDateTime out of Postgres range".into()))?;
        let mut buf = BytesMut::with_capacity(8);
        timestamp_to_sql(micros, &mut buf);
        params.push(Some(buf.to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::TIMESTAMP]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<NaiveDateTime> for ChronoTimestampCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<NaiveDateTime> {
        let bytes = cell(columns, "chrono::NaiveDateTime")?;
        let micros = timestamp_from_sql(bytes)
            .map_err(|e| Error::Codec(format!("chrono::NaiveDateTime: {e}")))?;
        Ok(pg_epoch() + Duration::microseconds(micros))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::TIMESTAMP]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Encoder<DateTime<Utc>> for ChronoDateTimeCodec {
    fn encode(&self, value: &DateTime<Utc>, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let micros = value
            .signed_duration_since(pg_epoch_utc())
            .num_microseconds()
            .ok_or_else(|| Error::Codec("chrono::DateTime<Utc> out of Postgres range".into()))?;
        let mut buf = BytesMut::with_capacity(8);
        timestamp_to_sql(micros, &mut buf);
        params.push(Some(buf.to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::TIMESTAMPTZ]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<DateTime<Utc>> for ChronoDateTimeCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<DateTime<Utc>> {
        let bytes = cell(columns, "chrono::DateTime<Utc>")?;
        let micros = timestamp_from_sql(bytes)
            .map_err(|e| Error::Codec(format!("chrono::DateTime<Utc>: {e}")))?;
        Ok(pg_epoch_utc() + Duration::microseconds(micros))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::TIMESTAMPTZ]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

fn cell<'a>(columns: &'a [Option<Bytes>], type_name: &str) -> Result<&'a [u8]> {
    columns
        .first()
        .ok_or_else(|| Error::Codec(format!("{type_name}: decoder needs 1 column, got 0")))?
        .as_deref()
        .ok_or_else(|| {
            Error::Codec(format!(
                "{type_name}: unexpected NULL; use nullable() to allow it"
            ))
        })
}

fn pg_epoch_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2000, 1, 1).expect("valid PG epoch")
}

fn pg_epoch() -> NaiveDateTime {
    pg_epoch_date()
        .and_hms_opt(0, 0, 0)
        .expect("valid PG epoch")
}

fn pg_epoch_utc() -> DateTime<Utc> {
    DateTime::from_naive_utc_and_offset(pg_epoch(), Utc)
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use proptest::prelude::*;

    use super::*;

    fn roundtrip<C, T>(codec: &C, value: &T) -> T
    where
        C: Encoder<T> + Decoder<T>,
        T: Clone,
    {
        let mut params = Vec::new();
        codec.encode(value, &mut params).unwrap();
        codec
            .decode(&[params.into_iter().next().unwrap().map(Bytes::from)])
            .unwrap()
    }

    proptest! {
        #[test]
        fn date_roundtrip(days in -30_000_i32..30_000_i32) {
            let value = pg_epoch_date() + Duration::days(i64::from(days));
            prop_assert_eq!(roundtrip(&chrono_date, &value), value);
        }

        #[test]
        fn time_roundtrip(hour in 0_u32..24, minute in 0_u32..60, second in 0_u32..60, micro in 0_u32..1_000_000) {
            let value = NaiveTime::from_hms_micro_opt(hour, minute, second, micro).unwrap();
            prop_assert_eq!(roundtrip(&chrono_time, &value), value);
        }

        #[test]
        fn timestamp_roundtrip(days in -10_000_i32..10_000_i32, second in 0_u32..86_400, micro in 0_u32..1_000_000) {
            let value = (pg_epoch_date() + Duration::days(i64::from(days))).and_hms_micro_opt(
                second / 3600,
                (second / 60) % 60,
                second % 60,
                micro,
            ).unwrap();
            prop_assert_eq!(roundtrip(&chrono_timestamp, &value), value);
        }

        #[test]
        fn timestamptz_roundtrip(days in -10_000_i32..10_000_i32, second in 0_u32..86_400, micro in 0_u32..1_000_000) {
            let naive = (pg_epoch_date() + Duration::days(i64::from(days))).and_hms_micro_opt(
                second / 3600,
                (second / 60) % 60,
                second % 60,
                micro,
            ).unwrap();
            let value = DateTime::from_naive_utc_and_offset(naive, Utc);
            prop_assert_eq!(roundtrip(&chrono_timestamptz, &value), value);
        }
    }
}
