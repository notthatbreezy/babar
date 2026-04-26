//! JSON codecs.

use std::marker::PhantomData;

use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use super::{Decoder, Encoder, FORMAT_TEXT};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Codec for `serde_json::Value` as Postgres `json`.
#[derive(Debug, Clone, Copy)]
pub struct JsonCodec;

/// Codec for `serde_json::Value` as Postgres `jsonb`.
#[derive(Debug, Clone, Copy)]
pub struct JsonbCodec;

/// Generic typed JSON codec backed by Postgres `jsonb`.
#[derive(Debug, Clone, Copy)]
pub struct TypedJsonCodec<T>(PhantomData<fn() -> T>);

/// Generic typed JSON codec backed by Postgres `json`.
#[derive(Debug, Clone, Copy)]
pub struct TypedJsonTextCodec<T>(PhantomData<fn() -> T>);

/// `json` codec value.
pub const json: JsonCodec = JsonCodec;
/// `jsonb` codec value.
pub const jsonb: JsonbCodec = JsonbCodec;

/// Build a typed `jsonb` codec for `T`.
pub fn typed_json<T>() -> TypedJsonCodec<T> {
    TypedJsonCodec(PhantomData)
}

/// Build a typed `json` codec for `T`.
pub fn typed_json_text<T>() -> TypedJsonTextCodec<T> {
    TypedJsonTextCodec(PhantomData)
}

impl Encoder<Value> for JsonCodec {
    fn encode(&self, value: &Value, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(
            serde_json::to_vec(value).map_err(|e| Error::Codec(format!("json: {e}")))?,
        ));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::JSON]
    }

    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Decoder<Value> for JsonCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Value> {
        decode_json::<Value>(columns, "json")
    }

    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::JSON]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Encoder<Value> for JsonbCodec {
    fn encode(&self, value: &Value, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(
            serde_json::to_vec(value).map_err(|e| Error::Codec(format!("jsonb: {e}")))?,
        ));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::JSONB]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl Decoder<Value> for JsonbCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Value> {
        decode_json::<Value>(columns, "jsonb")
    }

    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::JSONB]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl<T> Encoder<T> for TypedJsonCodec<T>
where
    T: Serialize,
{
    fn encode(&self, value: &T, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(
            serde_json::to_vec(value).map_err(|e| Error::Codec(format!("typed jsonb: {e}")))?,
        ));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::JSONB]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl<T> Decoder<T> for TypedJsonCodec<T>
where
    T: DeserializeOwned,
{
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<T> {
        decode_json::<T>(columns, "typed jsonb")
    }

    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::JSONB]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl<T> Encoder<T> for TypedJsonTextCodec<T>
where
    T: Serialize,
{
    fn encode(&self, value: &T, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(
            serde_json::to_vec(value).map_err(|e| Error::Codec(format!("typed json: {e}")))?,
        ));
        Ok(())
    }

    fn oids(&self) -> &'static [Oid] {
        &[types::JSON]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

impl<T> Decoder<T> for TypedJsonTextCodec<T>
where
    T: DeserializeOwned,
{
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<T> {
        decode_json::<T>(columns, "typed json")
    }

    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::JSON]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_TEXT]
    }
}

fn decode_json<T>(columns: &[Option<Bytes>], kind: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let bytes = columns
        .first()
        .ok_or_else(|| Error::Codec(format!("{kind}: decoder needs 1 column, got 0")))?
        .as_deref()
        .ok_or_else(|| {
            Error::Codec(format!(
                "{kind}: unexpected NULL; use nullable() to allow it"
            ))
        })?;
    serde_json::from_slice(bytes).map_err(|e| Error::Codec(format!("{kind}: {e}")))
}
