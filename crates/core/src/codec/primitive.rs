//! Primitive text-format codecs for M1.
//!
//! Each codec is a unit struct plus a public lowercase const that's the
//! sole user-facing handle. Lowercase const names match Skunk and read
//! naturally in code; the `non_upper_case_globals` lint is allowed at
//! the module root in `mod.rs`.
//!
//! Text format only — every encoded parameter is the UTF-8 string
//! Postgres would print, every decoded column is parsed from UTF-8
//! bytes. M2 will add binary alongside.

use std::fmt::Write as _;
use std::str;

use bytes::Bytes;

use super::{Decoder, Encoder};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

/// Read the single column a primitive decoder consumes, producing a
/// non-NULL byte slice. Surfaces a clear error on `NULL` or empty slice.
fn primitive_bytes<'a>(columns: &'a [Option<Bytes>], type_name: &'static str) -> Result<&'a [u8]> {
    let cell = columns.first().ok_or_else(|| {
        Error::Codec(format!(
            "{type_name}: decoder needs 1 column, got 0; this is a driver bug if it reached you"
        ))
    })?;
    cell.as_deref()
        .ok_or_else(|| Error::Codec(format!("{type_name}: unexpected NULL; use nullable() to allow it")))
}

/// Read a primitive's bytes as `&str`.
fn primitive_str<'a>(columns: &'a [Option<Bytes>], type_name: &'static str) -> Result<&'a str> {
    let bytes = primitive_bytes(columns, type_name)?;
    str::from_utf8(bytes)
        .map_err(|e| Error::Codec(format!("{type_name}: column not UTF-8: {e}")))
}

/// Codec for `int2` / `smallint` / `i16`.
#[derive(Debug, Clone, Copy)]
pub struct Int2Codec;
/// `int2` codec value.
pub const int2: Int2Codec = Int2Codec;

impl Encoder<i16> for Int2Codec {
    fn encode(&self, value: &i16, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_string().into_bytes()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT2]
    }
}

impl Decoder<i16> for Int2Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<i16> {
        let s = primitive_str(columns, "int2")?;
        s.parse::<i16>()
            .map_err(|e| Error::Codec(format!("int2: {e} (got {s:?})")))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT2]
    }
}

/// Codec for `int4` / `int` / `i32`.
#[derive(Debug, Clone, Copy)]
pub struct Int4Codec;
/// `int4` codec value.
pub const int4: Int4Codec = Int4Codec;

impl Encoder<i32> for Int4Codec {
    fn encode(&self, value: &i32, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_string().into_bytes()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT4]
    }
}

impl Decoder<i32> for Int4Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<i32> {
        let s = primitive_str(columns, "int4")?;
        s.parse::<i32>()
            .map_err(|e| Error::Codec(format!("int4: {e} (got {s:?})")))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT4]
    }
}

/// Codec for `int8` / `bigint` / `i64`.
#[derive(Debug, Clone, Copy)]
pub struct Int8Codec;
/// `int8` codec value.
pub const int8: Int8Codec = Int8Codec;

impl Encoder<i64> for Int8Codec {
    fn encode(&self, value: &i64, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_string().into_bytes()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT8]
    }
}

impl Decoder<i64> for Int8Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<i64> {
        let s = primitive_str(columns, "int8")?;
        s.parse::<i64>()
            .map_err(|e| Error::Codec(format!("int8: {e} (got {s:?})")))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT8]
    }
}

/// Codec for `float4` / `real` / `f32`.
#[derive(Debug, Clone, Copy)]
pub struct Float4Codec;
/// `float4` codec value.
pub const float4: Float4Codec = Float4Codec;

impl Encoder<f32> for Float4Codec {
    fn encode(&self, value: &f32, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(format_float(f64::from(*value)).into_bytes()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::FLOAT4]
    }
}

impl Decoder<f32> for Float4Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<f32> {
        let s = primitive_str(columns, "float4")?;
        parse_float_f32(s)
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::FLOAT4]
    }
}

/// Codec for `float8` / `double precision` / `f64`.
#[derive(Debug, Clone, Copy)]
pub struct Float8Codec;
/// `float8` codec value.
pub const float8: Float8Codec = Float8Codec;

impl Encoder<f64> for Float8Codec {
    fn encode(&self, value: &f64, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(format_float(*value).into_bytes()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::FLOAT8]
    }
}

impl Decoder<f64> for Float8Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<f64> {
        let s = primitive_str(columns, "float8")?;
        parse_float_f64(s)
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::FLOAT8]
    }
}

/// Codec for `bool`.
#[derive(Debug, Clone, Copy)]
pub struct BoolCodec;
/// `bool` codec value.
pub const bool: BoolCodec = BoolCodec;

impl Encoder<core::primitive::bool> for BoolCodec {
    fn encode(&self, value: &core::primitive::bool, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        // PG accepts 'true'/'false', 't'/'f', '1'/'0'. We send the
        // canonical 't'/'f' form for compactness.
        params.push(Some(if *value { b"t".to_vec() } else { b"f".to_vec() }));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::BOOL]
    }
}

impl Decoder<core::primitive::bool> for BoolCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<core::primitive::bool> {
        // PG returns 't' or 'f' in text format.
        let bytes = primitive_bytes(columns, "bool")?;
        match bytes {
            b"t" | b"true" | b"1" => Ok(true),
            b"f" | b"false" | b"0" => Ok(false),
            other => Err(Error::Codec(format!(
                "bool: unexpected value {:?}",
                String::from_utf8_lossy(other)
            ))),
        }
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::BOOL]
    }
}

/// Codec for `text`.
#[derive(Debug, Clone, Copy)]
pub struct TextCodec;
/// `text` codec value.
pub const text: TextCodec = TextCodec;

impl Encoder<String> for TextCodec {
    fn encode(&self, value: &String, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.as_bytes().to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::TEXT]
    }
}

impl Decoder<String> for TextCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<String> {
        primitive_str(columns, "text").map(ToString::to_string)
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::TEXT]
    }
}

/// Codec for `varchar`.
///
/// Encoded the same way as `text` — Postgres treats them
/// interchangeably in text format. The OIDs differ.
#[derive(Debug, Clone, Copy)]
pub struct VarcharCodec;
/// `varchar` codec value.
pub const varchar: VarcharCodec = VarcharCodec;

impl Encoder<String> for VarcharCodec {
    fn encode(&self, value: &String, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.as_bytes().to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::VARCHAR]
    }
}

impl Decoder<String> for VarcharCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<String> {
        primitive_str(columns, "varchar").map(ToString::to_string)
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::VARCHAR]
    }
}

/// Codec for `bpchar` / `char(N)` (blank-padded).
///
/// Postgres pads `char(N)` columns with spaces; we return the
/// padded value verbatim. Trim manually if you need the unpadded form.
#[derive(Debug, Clone, Copy)]
pub struct BpcharCodec;
/// `bpchar` codec value.
pub const bpchar: BpcharCodec = BpcharCodec;

impl Encoder<String> for BpcharCodec {
    fn encode(&self, value: &String, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.as_bytes().to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::BPCHAR]
    }
}

impl Decoder<String> for BpcharCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<String> {
        primitive_str(columns, "bpchar").map(ToString::to_string)
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::BPCHAR]
    }
}

/// Codec for `bytea`.
///
/// Text-format `bytea` is the `\x` hex form: `'\xDEADBEEF'` round-trips
/// to `[0xDE, 0xAD, 0xBE, 0xEF]`. Older `escape` format is not produced
/// by current Postgres servers and is not parsed here.
#[derive(Debug, Clone, Copy)]
pub struct ByteaCodec;
/// `bytea` codec value.
pub const bytea: ByteaCodec = ByteaCodec;

impl Encoder<Vec<u8>> for ByteaCodec {
    fn encode(&self, value: &Vec<u8>, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        let mut hex = String::with_capacity(2 + value.len() * 2);
        hex.push_str("\\x");
        for byte in value {
            let _ = write!(hex, "{byte:02x}");
        }
        params.push(Some(hex.into_bytes()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::BYTEA]
    }
}

impl Decoder<Vec<u8>> for ByteaCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Vec<u8>> {
        let s = primitive_str(columns, "bytea")?;
        let hex = s
            .strip_prefix("\\x")
            .ok_or_else(|| Error::Codec(format!("bytea: expected \\x prefix, got {s:?}")))?;
        if hex.len() % 2 != 0 {
            return Err(Error::Codec(format!("bytea: odd hex length in {s:?}")));
        }
        let mut out = Vec::with_capacity(hex.len() / 2);
        for chunk in hex.as_bytes().chunks(2) {
            let pair = str::from_utf8(chunk).map_err(|_| Error::Codec("bytea: non-UTF8".into()))?;
            let b = u8::from_str_radix(pair, 16)
                .map_err(|_| Error::Codec(format!("bytea: bad hex digit pair {pair:?}")))?;
            out.push(b);
        }
        Ok(out)
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::BYTEA]
    }
}

/// Format a float in a way that survives a Postgres roundtrip — Rust's
/// default float-to-string can drop trailing zeros that PG expects, and
/// hex-float syntax is right out. `{:?}` works because Rust's Debug for
/// floats always uses decimal notation and round-trips losslessly.
fn format_float(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        }
    } else {
        format!("{value:?}")
    }
}

fn parse_float_f64(s: &str) -> Result<f64> {
    match s {
        "NaN" => Ok(f64::NAN),
        "Infinity" => Ok(f64::INFINITY),
        "-Infinity" => Ok(f64::NEG_INFINITY),
        other => other
            .parse::<f64>()
            .map_err(|e| Error::Codec(format!("float8: {e} (got {s:?})"))),
    }
}

fn parse_float_f32(s: &str) -> Result<f32> {
    match s {
        "NaN" => Ok(f32::NAN),
        "Infinity" => Ok(f32::INFINITY),
        "-Infinity" => Ok(f32::NEG_INFINITY),
        other => other
            .parse::<f32>()
            .map_err(|e| Error::Codec(format!("float4: {e} (got {s:?})"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode<C: Encoder<A>, A>(c: &C, v: &A) -> Vec<Option<Vec<u8>>> {
        let mut out = Vec::new();
        c.encode(v, &mut out).expect("encode");
        out
    }

    fn one_col(v: &[u8]) -> Vec<Option<Bytes>> {
        vec![Some(Bytes::copy_from_slice(v))]
    }

    #[test]
    fn int4_roundtrip_boundaries() {
        for v in [0_i32, 1, -1, i32::MAX, i32::MIN] {
            let params = encode(&int4, &v);
            assert_eq!(params.len(), 1);
            let bytes = params[0].clone().unwrap();
            let decoded = int4
                .decode(&[Some(Bytes::from(bytes))])
                .expect("decode");
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn int2_roundtrip_boundaries() {
        for v in [0_i16, 1, -1, i16::MAX, i16::MIN] {
            let params = encode(&int2, &v);
            let decoded = int2
                .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
                .unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn int8_roundtrip_boundaries() {
        for v in [0_i64, 1, -1, i64::MAX, i64::MIN] {
            let params = encode(&int8, &v);
            let decoded = int8
                .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
                .unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn bool_roundtrip() {
        for v in [true, false] {
            let params = encode(&bool, &v);
            let decoded = bool
                .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
                .unwrap();
            assert_eq!(decoded, v);
        }
        // PG also returns these forms in some legacy contexts.
        assert!(bool.decode(&one_col(b"true")).unwrap());
        assert!(!bool.decode(&one_col(b"false")).unwrap());
        assert!(bool.decode(&one_col(b"1")).unwrap());
        assert!(!bool.decode(&one_col(b"0")).unwrap());
    }

    #[test]
    fn text_roundtrip_includes_empty() {
        for v in ["", "hello", "naïve\0nul", "🦀 unicode"] {
            let s = v.to_string();
            let params = encode(&text, &s);
            let decoded = text
                .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
                .unwrap();
            assert_eq!(decoded, s);
        }
    }

    #[test]
    fn float8_handles_specials() {
        let nan = float8.decode(&one_col(b"NaN")).unwrap();
        assert!(nan.is_nan());
        assert!(float8.decode(&one_col(b"Infinity")).unwrap().is_infinite());
        assert!(float8
            .decode(&one_col(b"-Infinity"))
            .unwrap()
            .is_sign_negative());
        // Float roundtrip via Debug formatting is bit-exact, so direct
        // comparison is intentional here.
        for bits in [
            0_f64.to_bits(),
            1.5_f64.to_bits(),
            (-2.5_f64).to_bits(),
            f64::MIN_POSITIVE.to_bits(),
            f64::MAX.to_bits(),
        ] {
            let v = f64::from_bits(bits);
            let params = encode(&float8, &v);
            let decoded = float8
                .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
                .unwrap();
            assert_eq!(decoded.to_bits(), bits, "float8 bit-exact roundtrip");
        }
    }

    #[test]
    fn float4_handles_specials() {
        let nan = float4.decode(&one_col(b"NaN")).unwrap();
        assert!(nan.is_nan());
        for bits in [0_f32.to_bits(), 1.5_f32.to_bits(), (-2.5_f32).to_bits()] {
            let v = f32::from_bits(bits);
            let params = encode(&float4, &v);
            let decoded = float4
                .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
                .unwrap();
            assert_eq!(decoded.to_bits(), bits);
        }
    }

    #[test]
    fn bytea_roundtrip() {
        for v in [
            Vec::<u8>::new(),
            vec![0x00, 0xFF],
            vec![0xDE, 0xAD, 0xBE, 0xEF],
            (0..=255_u8).collect(),
        ] {
            let params = encode(&bytea, &v);
            let decoded = bytea
                .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
                .unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn null_into_primitive_decoder_errors() {
        let err = int4.decode(&[None]).unwrap_err();
        match err {
            Error::Codec(msg) => assert!(msg.contains("NULL"), "unexpected: {msg}"),
            other => panic!("wrong error: {other:?}"),
        }
    }
}
