//! `time` crate codecs.

use bytes::{Bytes, BytesMut};
use postgres_protocol::types::{
    date_from_sql, date_to_sql, time_from_sql, time_to_sql, timestamp_from_sql, timestamp_to_sql,
};

use super::{Decoder, Encoder, FORMAT_BINARY};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Codec for `time::Date`.
#[derive(Debug, Clone, Copy)]
pub struct DateCodec;
/// Codec for `time::Time`.
#[derive(Debug, Clone, Copy)]
pub struct TimeCodec;
/// Codec for `time::PrimitiveDateTime`.
#[derive(Debug, Clone, Copy)]
pub struct PrimitiveDateTimeCodec;
/// Codec for `time::OffsetDateTime`.
#[derive(Debug, Clone, Copy)]
pub struct OffsetDateTimeCodec;

/// `date` codec value.
pub const date: DateCodec = DateCodec;
/// `time` codec value.
pub const time: TimeCodec = TimeCodec;
/// `timestamp` codec value.
pub const timestamp: PrimitiveDateTimeCodec = PrimitiveDateTimeCodec;
/// `timestamptz` codec value.
pub const timestamptz: OffsetDateTimeCodec = OffsetDateTimeCodec;

impl Encoder<::time::Date> for DateCodec {
    fn encode(&self, value: &::time::Date, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let days = (*value - pg_epoch_date()).whole_days();
        let days = i32::try_from(days)
            .map_err(|_| Error::Codec("time::Date out of Postgres range".into()))?;
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

impl Decoder<::time::Date> for DateCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<::time::Date> {
        let bytes = cell(columns, "time::Date")?;
        let days = date_from_sql(bytes).map_err(|e| Error::Codec(format!("time::Date: {e}")))?;
        Ok(pg_epoch_date() + ::time::Duration::days(i64::from(days)))
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

impl Encoder<::time::Time> for TimeCodec {
    fn encode(&self, value: &::time::Time, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let micros = i64::from(value.hour()) * 3_600_000_000
            + i64::from(value.minute()) * 60_000_000
            + i64::from(value.second()) * 1_000_000
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

impl Decoder<::time::Time> for TimeCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<::time::Time> {
        let bytes = cell(columns, "time::Time")?;
        let micros = time_from_sql(bytes).map_err(|e| Error::Codec(format!("time::Time: {e}")))?;
        let hour = u8::try_from(micros.div_euclid(3_600_000_000))
            .map_err(|_| Error::Codec("time::Time: hour out of range".into()))?;
        let minute = u8::try_from(micros.div_euclid(60_000_000) % 60)
            .map_err(|_| Error::Codec("time::Time: minute out of range".into()))?;
        let second = u8::try_from(micros.div_euclid(1_000_000) % 60)
            .map_err(|_| Error::Codec("time::Time: second out of range".into()))?;
        let nano = u32::try_from(micros.rem_euclid(1_000_000) * 1_000)
            .map_err(|_| Error::Codec("time::Time: nanosecond out of range".into()))?;
        ::time::Time::from_hms_nano(hour, minute, second, nano)
            .map_err(|e| Error::Codec(format!("time::Time: {e}")))
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

impl Encoder<::time::PrimitiveDateTime> for PrimitiveDateTimeCodec {
    fn encode(
        &self,
        value: &::time::PrimitiveDateTime,
        params: &mut Vec<Option<Vec<u8>>>,
    ) -> Result<()> {
        let micros = (*value - pg_epoch()).whole_microseconds();
        let micros = i64::try_from(micros)
            .map_err(|_| Error::Codec("time::PrimitiveDateTime out of Postgres range".into()))?;
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

impl Decoder<::time::PrimitiveDateTime> for PrimitiveDateTimeCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<::time::PrimitiveDateTime> {
        let bytes = cell(columns, "time::PrimitiveDateTime")?;
        let micros = timestamp_from_sql(bytes)
            .map_err(|e| Error::Codec(format!("time::PrimitiveDateTime: {e}")))?;
        Ok(pg_epoch() + ::time::Duration::microseconds(micros))
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

impl Encoder<::time::OffsetDateTime> for OffsetDateTimeCodec {
    fn encode(
        &self,
        value: &::time::OffsetDateTime,
        params: &mut Vec<Option<Vec<u8>>>,
    ) -> Result<()> {
        let utc = value.to_offset(::time::UtcOffset::UTC);
        let micros = (utc - pg_epoch_utc()).whole_microseconds();
        let micros = i64::try_from(micros)
            .map_err(|_| Error::Codec("time::OffsetDateTime out of Postgres range".into()))?;
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

impl Decoder<::time::OffsetDateTime> for OffsetDateTimeCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<::time::OffsetDateTime> {
        let bytes = cell(columns, "time::OffsetDateTime")?;
        let micros = timestamp_from_sql(bytes)
            .map_err(|e| Error::Codec(format!("time::OffsetDateTime: {e}")))?;
        Ok(pg_epoch_utc() + ::time::Duration::microseconds(micros))
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

fn pg_epoch_date() -> ::time::Date {
    ::time::Date::from_calendar_date(2000, ::time::Month::January, 1).expect("valid PG epoch")
}

fn pg_epoch() -> ::time::PrimitiveDateTime {
    pg_epoch_date().with_time(::time::Time::MIDNIGHT)
}

fn pg_epoch_utc() -> ::time::OffsetDateTime {
    pg_epoch().assume_utc()
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
            let value = pg_epoch_date() + ::time::Duration::days(i64::from(days));
            prop_assert_eq!(roundtrip(&date, &value), value);
        }

        #[test]
        fn time_roundtrip(hour in 0_u8..24, minute in 0_u8..60, second in 0_u8..60, micro in 0_u32..1_000_000) {
            let value = ::time::Time::from_hms_micro(hour, minute, second, micro).unwrap();
            prop_assert_eq!(roundtrip(&time, &value), value);
        }

        #[test]
        fn primitive_datetime_roundtrip(days in -10_000_i32..10_000_i32, second in 0_u32..86_400, micro in 0_u32..1_000_000) {
            let date_part = pg_epoch_date() + ::time::Duration::days(i64::from(days));
            let value = date_part.with_time(::time::Time::from_hms_micro(
                u8::try_from(second / 3600).unwrap(),
                u8::try_from((second / 60) % 60).unwrap(),
                u8::try_from(second % 60).unwrap(),
                micro,
            ).unwrap());
            prop_assert_eq!(roundtrip(&timestamp, &value), value);
        }

        #[test]
        fn offset_datetime_roundtrip(days in -10_000_i32..10_000_i32, second in 0_u32..86_400, micro in 0_u32..1_000_000) {
            let date_part = pg_epoch_date() + ::time::Duration::days(i64::from(days));
            let value = date_part.with_time(::time::Time::from_hms_micro(
                u8::try_from(second / 3600).unwrap(),
                u8::try_from((second / 60) % 60).unwrap(),
                u8::try_from(second % 60).unwrap(),
                micro,
            ).unwrap()).assume_utc();
            prop_assert_eq!(roundtrip(&timestamptz, &value), value);
        }
    }
}
