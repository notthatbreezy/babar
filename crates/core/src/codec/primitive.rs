//! Primitive codecs with binary and text format support.
//!
//! Each codec is a unit struct plus a public lowercase const that's the
//! sole user-facing handle. Lowercase const names match Skunk and read
//! naturally in code; the `non_upper_case_globals` lint is allowed at
//! the module root in `mod.rs`.
//!
//! All primitive codecs support binary format (format code `1`) and use
//! it by default. The driver sends binary format codes in the Bind
//! message and receives binary data in `DataRow`. Text format is retained
//! as a fallback (e.g., when the server returns text from simple query).

use std::str;

use bytes::Bytes;

use super::{Decoder, Encoder, FORMAT_BINARY};
use crate::error::{Error, Result};
use crate::types::{self, Oid};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Read the single column a primitive decoder consumes, producing a
/// non-NULL byte slice. Surfaces a clear error on `NULL` or empty slice.
fn primitive_bytes<'a>(columns: &'a [Option<Bytes>], type_name: &'static str) -> Result<&'a [u8]> {
    let cell = columns.first().ok_or_else(|| {
        Error::Codec(format!(
            "{type_name}: decoder needs 1 column, got 0; this is a driver bug if it reached you"
        ))
    })?;
    cell.as_deref().ok_or_else(|| {
        Error::Codec(format!(
            "{type_name}: unexpected NULL; use nullable() to allow it"
        ))
    })
}

/// Read a primitive's bytes as `&str`.
fn primitive_str<'a>(columns: &'a [Option<Bytes>], type_name: &'static str) -> Result<&'a str> {
    let bytes = primitive_bytes(columns, type_name)?;
    str::from_utf8(bytes).map_err(|e| Error::Codec(format!("{type_name}: column not UTF-8: {e}")))
}

// ---------------------------------------------------------------------------
// int2 — Postgres `smallint`, Rust `i16`
// ---------------------------------------------------------------------------

/// Codec for `int2` / `smallint` / `i16`.
#[derive(Debug, Clone, Copy)]
pub struct Int2Codec;
/// `int2` codec value.
pub const int2: Int2Codec = Int2Codec;

impl Encoder<i16> for Int2Codec {
    fn encode(&self, value: &i16, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_be_bytes().to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT2]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<i16> for Int2Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<i16> {
        let bytes = primitive_bytes(columns, "int2")?;
        if bytes.len() == 2 {
            return Ok(i16::from_be_bytes([bytes[0], bytes[1]]));
        }
        // Text fallback (simple-query or legacy).
        let s = str::from_utf8(bytes)
            .map_err(|e| Error::Codec(format!("int2: column not UTF-8: {e}")))?;
        s.parse::<i16>()
            .map_err(|e| Error::Codec(format!("int2: {e} (got {s:?})")))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT2]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

// ---------------------------------------------------------------------------
// int4 — Postgres `integer`, Rust `i32`
// ---------------------------------------------------------------------------

/// Codec for `int4` / `int` / `i32`.
#[derive(Debug, Clone, Copy)]
pub struct Int4Codec;
/// `int4` codec value.
pub const int4: Int4Codec = Int4Codec;

impl Encoder<i32> for Int4Codec {
    fn encode(&self, value: &i32, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_be_bytes().to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT4]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<i32> for Int4Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<i32> {
        let bytes = primitive_bytes(columns, "int4")?;
        // Binary: 4 bytes big-endian.
        if bytes.len() == 4 {
            return Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        }
        // Text fallback.
        let s = str::from_utf8(bytes)
            .map_err(|e| Error::Codec(format!("int4: column not UTF-8: {e}")))?;
        s.parse::<i32>()
            .map_err(|e| Error::Codec(format!("int4: {e} (got {s:?})")))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT4]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

// ---------------------------------------------------------------------------
// int8 — Postgres `bigint`, Rust `i64`
// ---------------------------------------------------------------------------

/// Codec for `int8` / `bigint` / `i64`.
#[derive(Debug, Clone, Copy)]
pub struct Int8Codec;
/// `int8` codec value.
pub const int8: Int8Codec = Int8Codec;

impl Encoder<i64> for Int8Codec {
    fn encode(&self, value: &i64, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_be_bytes().to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT8]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<i64> for Int8Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<i64> {
        let bytes = primitive_bytes(columns, "int8")?;
        // Binary: 8 bytes big-endian.
        if bytes.len() == 8 {
            let arr: [u8; 8] = bytes[..8].try_into().expect("len checked");
            return Ok(i64::from_be_bytes(arr));
        }
        // Text fallback.
        let s = str::from_utf8(bytes)
            .map_err(|e| Error::Codec(format!("int8: column not UTF-8: {e}")))?;
        s.parse::<i64>()
            .map_err(|e| Error::Codec(format!("int8: {e} (got {s:?})")))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::INT8]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

// ---------------------------------------------------------------------------
// float4 — Postgres `real`, Rust `f32`
// ---------------------------------------------------------------------------

/// Codec for `float4` / `real` / `f32`.
#[derive(Debug, Clone, Copy)]
pub struct Float4Codec;
/// `float4` codec value.
pub const float4: Float4Codec = Float4Codec;

impl Encoder<f32> for Float4Codec {
    fn encode(&self, value: &f32, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_be_bytes().to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::FLOAT4]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<f32> for Float4Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<f32> {
        let bytes = primitive_bytes(columns, "float4")?;
        // Try text first: if it's valid UTF-8 and parses as a float, use that.
        // This handles both text-format results and binary-format results
        // since 4 random bytes are unlikely to be valid decimal text.
        if let Ok(s) = str::from_utf8(bytes) {
            if let Ok(v) = parse_float_f32(s) {
                return Ok(v);
            }
        }
        // Binary: 4 bytes IEEE 754 big-endian.
        if bytes.len() == 4 {
            return Ok(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        }
        Err(Error::Codec(format!(
            "float4: cannot decode {} bytes as text or binary",
            bytes.len()
        )))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::FLOAT4]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

// ---------------------------------------------------------------------------
// float8 — Postgres `double precision`, Rust `f64`
// ---------------------------------------------------------------------------

/// Codec for `float8` / `double precision` / `f64`.
#[derive(Debug, Clone, Copy)]
pub struct Float8Codec;
/// `float8` codec value.
pub const float8: Float8Codec = Float8Codec;

impl Encoder<f64> for Float8Codec {
    fn encode(&self, value: &f64, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        params.push(Some(value.to_be_bytes().to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::FLOAT8]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<f64> for Float8Codec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<f64> {
        let bytes = primitive_bytes(columns, "float8")?;
        // Try text first: if it's valid UTF-8 and parses as a float, use that.
        if let Ok(s) = str::from_utf8(bytes) {
            if let Ok(v) = parse_float_f64(s) {
                return Ok(v);
            }
        }
        // Binary: 8 bytes IEEE 754 big-endian.
        if bytes.len() == 8 {
            let arr: [u8; 8] = bytes[..8].try_into().expect("len checked");
            return Ok(f64::from_be_bytes(arr));
        }
        Err(Error::Codec(format!(
            "float8: cannot decode {} bytes as text or binary",
            bytes.len()
        )))
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::FLOAT8]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

// ---------------------------------------------------------------------------
// bool — Postgres `boolean`, Rust `bool`
// ---------------------------------------------------------------------------

/// Codec for `bool`.
#[derive(Debug, Clone, Copy)]
pub struct BoolCodec;
/// `bool` codec value.
pub const bool: BoolCodec = BoolCodec;

impl Encoder<core::primitive::bool> for BoolCodec {
    fn encode(
        &self,
        value: &core::primitive::bool,
        params: &mut Vec<Option<Vec<u8>>>,
    ) -> Result<()> {
        // Binary: single byte, 0x01 = true, 0x00 = false.
        params.push(Some(vec![u8::from(*value)]));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::BOOL]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<core::primitive::bool> for BoolCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<core::primitive::bool> {
        let bytes = primitive_bytes(columns, "bool")?;
        // Binary: single byte.
        if bytes.len() == 1 {
            return match bytes[0] {
                1 | b't' => Ok(true),
                0 | b'f' => Ok(false),
                other => Err(Error::Codec(format!("bool: unexpected byte {other:#04x}"))),
            };
        }
        // Text fallback: multi-byte strings.
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
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

// ---------------------------------------------------------------------------
// text — Postgres `text`, Rust `String`
// ---------------------------------------------------------------------------

/// Codec for `text`.
#[derive(Debug, Clone, Copy)]
pub struct TextCodec;
/// `text` codec value.
pub const text: TextCodec = TextCodec;

impl Encoder<String> for TextCodec {
    fn encode(&self, value: &String, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        // Binary format for text is just raw UTF-8 bytes (same as text format).
        params.push(Some(value.as_bytes().to_vec()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::TEXT]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<String> for TextCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<String> {
        // Both binary and text: raw UTF-8 bytes.
        primitive_str(columns, "text").map(ToString::to_string)
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::TEXT]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

// ---------------------------------------------------------------------------
// varchar — Postgres `character varying`
// ---------------------------------------------------------------------------

/// Codec for `varchar`.
///
/// Encoded identically to `text` in both text and binary format; the OIDs differ.
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
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
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
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

// ---------------------------------------------------------------------------
// bpchar — Postgres `char(N)` (blank-padded)
// ---------------------------------------------------------------------------

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
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
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
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

// ---------------------------------------------------------------------------
// bytea — Postgres `bytea`, Rust `Vec<u8>`
// ---------------------------------------------------------------------------

/// Codec for `bytea`.
///
/// Binary format sends raw bytes directly; text format uses the `\x`
/// hex encoding. The encoder always uses binary.
#[derive(Debug, Clone, Copy)]
pub struct ByteaCodec;
/// `bytea` codec value.
pub const bytea: ByteaCodec = ByteaCodec;

impl Encoder<Vec<u8>> for ByteaCodec {
    fn encode(&self, value: &Vec<u8>, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
        // Binary: raw bytes.
        params.push(Some(value.clone()));
        Ok(())
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::BYTEA]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

impl Decoder<Vec<u8>> for ByteaCodec {
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<Vec<u8>> {
        let bytes = primitive_bytes(columns, "bytea")?;
        // If it starts with `\x`, it's text-format hex encoding.
        if bytes.starts_with(b"\\x") {
            let s = str::from_utf8(bytes)
                .map_err(|e| Error::Codec(format!("bytea: text not UTF-8: {e}")))?;
            let hex = &s[2..];
            if hex.len() % 2 != 0 {
                return Err(Error::Codec(format!("bytea: odd hex length in {s:?}")));
            }
            let mut out = Vec::with_capacity(hex.len() / 2);
            for chunk in hex.as_bytes().chunks(2) {
                let pair =
                    str::from_utf8(chunk).map_err(|_| Error::Codec("bytea: non-UTF8".into()))?;
                let b = u8::from_str_radix(pair, 16)
                    .map_err(|_| Error::Codec(format!("bytea: bad hex digit pair {pair:?}")))?;
                out.push(b);
            }
            return Ok(out);
        }
        // Binary format: raw bytes.
        Ok(bytes.to_vec())
    }
    fn n_columns(&self) -> usize {
        1
    }
    fn oids(&self) -> &'static [Oid] {
        &[types::BYTEA]
    }
    fn format_codes(&self) -> &'static [i16] {
        &[FORMAT_BINARY]
    }
}

// ---------------------------------------------------------------------------
// Helpers (text format)
// ---------------------------------------------------------------------------

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

    fn encode_val<C: Encoder<A>, A>(c: &C, v: &A) -> Vec<Option<Vec<u8>>> {
        let mut out = Vec::new();
        c.encode(v, &mut out).expect("encode");
        out
    }

    fn one_col(v: &[u8]) -> Vec<Option<Bytes>> {
        vec![Some(Bytes::copy_from_slice(v))]
    }

    // --- Binary roundtrips -----------------------------------------------

    #[test]
    fn int2_binary_roundtrip() {
        for v in [0_i16, 1, -1, i16::MAX, i16::MIN] {
            let params = encode_val(&int2, &v);
            assert_eq!(params.len(), 1);
            let bytes = params[0].clone().unwrap();
            assert_eq!(bytes.len(), 2, "int2 binary is 2 bytes");
            let decoded = int2.decode(&[Some(Bytes::from(bytes))]).unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn int4_binary_roundtrip() {
        for v in [0_i32, 1, -1, i32::MAX, i32::MIN] {
            let params = encode_val(&int4, &v);
            assert_eq!(params.len(), 1);
            let bytes = params[0].clone().unwrap();
            assert_eq!(bytes.len(), 4, "int4 binary is 4 bytes");
            let decoded = int4.decode(&[Some(Bytes::from(bytes))]).unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn int8_binary_roundtrip() {
        for v in [0_i64, 1, -1, i64::MAX, i64::MIN] {
            let params = encode_val(&int8, &v);
            assert_eq!(params.len(), 1);
            let bytes = params[0].clone().unwrap();
            assert_eq!(bytes.len(), 8, "int8 binary is 8 bytes");
            let decoded = int8.decode(&[Some(Bytes::from(bytes))]).unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn float4_binary_roundtrip() {
        for v in [0.0_f32, 1.5, -2.5, f32::MIN_POSITIVE, f32::MAX] {
            let params = encode_val(&float4, &v);
            let bytes = params[0].clone().unwrap();
            assert_eq!(bytes.len(), 4, "float4 binary is 4 bytes");
            let decoded = float4.decode(&[Some(Bytes::from(bytes))]).unwrap();
            assert_eq!(decoded.to_bits(), v.to_bits());
        }
        // NaN roundtrip
        let params = encode_val(&float4, &f32::NAN);
        let decoded = float4
            .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
            .unwrap();
        assert!(decoded.is_nan());
    }

    #[test]
    fn float8_binary_roundtrip() {
        for v in [0.0_f64, 1.5, -2.5, f64::MIN_POSITIVE, f64::MAX] {
            let params = encode_val(&float8, &v);
            let bytes = params[0].clone().unwrap();
            assert_eq!(bytes.len(), 8, "float8 binary is 8 bytes");
            let decoded = float8.decode(&[Some(Bytes::from(bytes))]).unwrap();
            assert_eq!(decoded.to_bits(), v.to_bits());
        }
        let params = encode_val(&float8, &f64::NAN);
        let decoded = float8
            .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
            .unwrap();
        assert!(decoded.is_nan());
    }

    #[test]
    fn bool_binary_roundtrip() {
        for v in [true, false] {
            let params = encode_val(&bool, &v);
            let bytes = params[0].clone().unwrap();
            assert_eq!(bytes.len(), 1, "bool binary is 1 byte");
            let decoded = bool.decode(&[Some(Bytes::from(bytes))]).unwrap();
            assert_eq!(decoded, v);
        }
    }

    #[test]
    fn text_binary_roundtrip() {
        for v in ["", "hello", "naïve\0nul", "🦀 unicode"] {
            let s = v.to_string();
            let params = encode_val(&text, &s);
            let decoded = text
                .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
                .unwrap();
            assert_eq!(decoded, s);
        }
    }

    #[test]
    fn bytea_binary_roundtrip() {
        for v in [
            Vec::<u8>::new(),
            vec![0x00, 0xFF],
            vec![0xDE, 0xAD, 0xBE, 0xEF],
            (0..=255_u8).collect(),
        ] {
            let params = encode_val(&bytea, &v);
            // Binary: raw bytes (no \x prefix).
            assert_eq!(params[0].as_ref().unwrap(), &v);
            let decoded = bytea
                .decode(&[Some(Bytes::from(params[0].clone().unwrap()))])
                .unwrap();
            assert_eq!(decoded, v);
        }
    }

    // --- Text decode fallback (e.g., simple query results) ---------------

    #[test]
    fn int4_text_decode_fallback() {
        // Text-format bytes from a simple query.
        let decoded = int4.decode(&one_col(b"42")).unwrap();
        assert_eq!(decoded, 42);
        let decoded = int4.decode(&one_col(b"-1")).unwrap();
        assert_eq!(decoded, -1);
    }

    #[test]
    fn float8_text_decode_fallback() {
        assert!(float8.decode(&one_col(b"NaN")).unwrap().is_nan());
        assert!(float8.decode(&one_col(b"Infinity")).unwrap().is_infinite());
        assert!(float8
            .decode(&one_col(b"-Infinity"))
            .unwrap()
            .is_sign_negative());
    }

    #[test]
    fn bool_text_decode_fallback() {
        assert!(bool.decode(&one_col(b"true")).unwrap());
        assert!(!bool.decode(&one_col(b"false")).unwrap());
        assert!(bool.decode(&one_col(b"t")).unwrap());
        assert!(!bool.decode(&one_col(b"f")).unwrap());
    }

    #[test]
    fn bytea_text_decode_fallback() {
        // Text-format: hex-encoded with \x prefix.
        let decoded = bytea.decode(&one_col(b"\\xDEAD")).unwrap();
        assert_eq!(decoded, vec![0xDE, 0xAD]);
    }

    #[test]
    fn null_into_primitive_decoder_errors() {
        let err = int4.decode(&[None]).unwrap_err();
        match err {
            Error::Codec(msg) => assert!(msg.contains("NULL"), "unexpected: {msg}"),
            other => panic!("wrong error: {other:?}"),
        }
    }

    // --- Format code advertisement ---------------------------------------

    #[test]
    fn all_primitives_advertise_binary() {
        assert_eq!(Encoder::<i16>::format_codes(&int2), &[FORMAT_BINARY]);
        assert_eq!(Encoder::<i32>::format_codes(&int4), &[FORMAT_BINARY]);
        assert_eq!(Encoder::<i64>::format_codes(&int8), &[FORMAT_BINARY]);
        assert_eq!(Encoder::<f32>::format_codes(&float4), &[FORMAT_BINARY]);
        assert_eq!(Encoder::<f64>::format_codes(&float8), &[FORMAT_BINARY]);
        assert_eq!(
            Encoder::<core::primitive::bool>::format_codes(&bool),
            &[FORMAT_BINARY]
        );
        assert_eq!(Encoder::<String>::format_codes(&text), &[FORMAT_BINARY]);
        assert_eq!(Encoder::<String>::format_codes(&varchar), &[FORMAT_BINARY]);
        assert_eq!(Encoder::<String>::format_codes(&bpchar), &[FORMAT_BINARY]);
        assert_eq!(Encoder::<Vec<u8>>::format_codes(&bytea), &[FORMAT_BINARY]);
    }
}
