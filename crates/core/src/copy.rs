//! Typed `COPY FROM STDIN BINARY` support.
//!
//! `CopyIn<T>` is the bulk-ingest sibling of [`crate::query::Command`]: it
//! carries a SQL `COPY ... FROM STDIN BINARY` statement plus a dedicated row
//! encoder for `T`. COPY rows are framed differently from bind parameters, so
//! this module keeps that encoding path explicit even when it reuses existing
//! codec metadata.
//!
//! Supported today:
//! - binary `COPY ... FROM STDIN`
//! - in-memory row sources (`Vec<T>`, slices, iterators)
//! - tuple codecs and `#[derive(Codec)]` structs as row encoders
//!
//! Not supported yet:
//! - `COPY TO STDOUT`
//! - text or CSV COPY modes
//! - generalized streaming sources beyond `IntoIterator`
//!
//! ```no_run
//! use babar::query::Query;
//! use babar::{CopyIn, Session};
//!
//! #[derive(Clone, Debug, PartialEq, babar::Codec)]
//! struct UserRow {
//!     #[pg(codec = "int4")]
//!     id: i32,
//!     #[pg(codec = "text")]
//!     name: String,
//! }
//!
//! async fn demo(session: &Session) -> babar::Result<Vec<UserRow>> {
//!     let rows = vec![
//!         UserRow { id: 1, name: "Ada".into() },
//!         UserRow { id: 2, name: "Linus".into() },
//!     ];
//!     let copy = CopyIn::binary(
//!         "COPY users (id, name) FROM STDIN BINARY",
//!         UserRow::CODEC,
//!     );
//!     session.copy_in(&copy, rows).await?;
//!
//!     let select: Query<(), UserRow> =
//!         Query::raw("SELECT id, name FROM users ORDER BY id", (), UserRow::CODEC);
//!     session.query(&select, ()).await
//! }
//! ```

use std::borrow::Borrow;
use std::fmt;
use std::sync::Arc;

use bytes::{BufMut as _, Bytes, BytesMut};

use crate::codec::{Encoder, FORMAT_BINARY, FORMAT_TEXT};
use crate::error::{Error, Result};
use crate::protocol::frontend;
use crate::types::Oid;

/// A typed `COPY ... FROM STDIN BINARY` statement.
pub struct CopyIn<T> {
    sql: String,
    row_encoder: Arc<dyn CopyRowEncoder<T>>,
}

impl<T> Clone for CopyIn<T> {
    fn clone(&self) -> Self {
        Self {
            sql: self.sql.clone(),
            row_encoder: Arc::clone(&self.row_encoder),
        }
    }
}

impl<T> fmt::Debug for CopyIn<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CopyIn")
            .field("sql", &self.sql)
            .field("n_columns", &self.column_oids().len())
            .finish_non_exhaustive()
    }
}

impl<T> CopyIn<T> {
    /// Build a typed binary `COPY FROM STDIN` operation.
    ///
    /// `row_encoder` describes one COPY row of type `T`. Existing codecs work
    /// naturally here: tuple codecs cover tuple rows and `#[derive(Codec)]`
    /// structs can pass `MyRow::CODEC` directly.
    ///
    /// babar intentionally limits COPY support to binary `COPY FROM STDIN`
    /// bulk ingest. Text/CSV COPY modes and `COPY TO` stay out of scope for
    /// this API.
    pub fn binary<E>(sql: impl Into<String>, row_encoder: E) -> Self
    where
        E: Encoder<T> + Send + Sync + 'static,
    {
        Self {
            sql: sql.into(),
            row_encoder: Arc::new(EncoderCopyRowEncoder(row_encoder)),
        }
    }

    /// SQL text exactly as it will be sent to the server.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Postgres OIDs the row encoder declares, in COPY column order.
    pub fn column_oids(&self) -> &'static [Oid] {
        self.row_encoder.column_oids()
    }

    /// Number of columns each COPY row carries.
    pub fn n_columns(&self) -> usize {
        self.column_oids().len()
    }

    pub(crate) fn encode_rows<I, R>(&self, rows: I) -> Result<Vec<Bytes>>
    where
        I: IntoIterator<Item = R>,
        R: Borrow<T>,
    {
        BinaryCopyPayload::encode(self.row_encoder.as_ref(), rows)
    }
}

trait CopyRowEncoder<T>: Send + Sync {
    /// Encode one row into `row`.
    fn encode_row(&self, value: &T, row: &mut BinaryCopyRow) -> Result<()>;

    /// Postgres OIDs the row produces, in COPY column order.
    fn column_oids(&self) -> &'static [Oid];
}

struct EncoderCopyRowEncoder<E>(E);

impl<E, T> CopyRowEncoder<T> for EncoderCopyRowEncoder<E>
where
    E: Encoder<T> + Send + Sync,
{
    fn encode_row(&self, value: &T, row: &mut BinaryCopyRow) -> Result<()> {
        ensure_binary_copy_capable(self.0.oids(), self.0.format_codes())?;

        let mut fields = Vec::with_capacity(self.0.oids().len());
        self.0.encode(value, &mut fields)?;
        if fields.len() != self.0.oids().len() {
            return Err(Error::Codec(format!(
                "COPY row encoder declared {} columns but produced {} fields",
                self.0.oids().len(),
                fields.len()
            )));
        }

        row.encode_fields(&fields)
    }

    fn column_oids(&self) -> &'static [Oid] {
        self.0.oids()
    }
}

fn ensure_binary_copy_capable(column_oids: &[Oid], formats: &[i16]) -> Result<()> {
    for index in 0..column_oids.len() {
        let format = formats.get(index).copied().unwrap_or(FORMAT_TEXT);
        if format != FORMAT_BINARY {
            return Err(Error::Codec(format!(
                "COPY FROM STDIN BINARY requires binary row codecs; column {} uses format code {}",
                index + 1,
                format
            )));
        }
    }
    Ok(())
}

pub(crate) struct BinaryCopyRow {
    buf: BytesMut,
}

impl BinaryCopyRow {
    fn new() -> Self {
        Self {
            buf: BytesMut::new(),
        }
    }

    fn encode_fields(&mut self, fields: &[Option<Vec<u8>>]) -> Result<()> {
        let n_fields = i16::try_from(fields.len()).map_err(|_| {
            Error::Codec(format!("COPY row has too many columns: {}", fields.len()))
        })?;
        self.buf.clear();
        self.buf.put_i16(n_fields);
        for field in fields {
            match field {
                Some(bytes) => {
                    let len = i32::try_from(bytes.len()).map_err(|_| {
                        Error::Codec(format!(
                            "COPY field exceeds binary format limit: {} bytes",
                            bytes.len()
                        ))
                    })?;
                    self.buf.put_i32(len);
                    self.buf.extend_from_slice(bytes);
                }
                None => self.buf.put_i32(-1),
            }
        }
        Ok(())
    }

    fn freeze(&mut self) -> Bytes {
        self.buf.split().freeze()
    }
}

struct BinaryCopyPayload;

impl BinaryCopyPayload {
    fn encode<T, I, R>(row_encoder: &dyn CopyRowEncoder<T>, rows: I) -> Result<Vec<Bytes>>
    where
        I: IntoIterator<Item = R>,
        R: Borrow<T>,
    {
        let mut chunks = Vec::new();

        let mut header = BytesMut::new();
        frontend::copy_binary_header(&mut header);
        chunks.push(header.freeze());

        let mut row = BinaryCopyRow::new();
        for value in rows {
            row_encoder.encode_row(value.borrow(), &mut row)?;
            chunks.push(row.freeze());
        }

        let mut trailer = BytesMut::new();
        frontend::copy_binary_trailer(&mut trailer);
        chunks.push(trailer.freeze());

        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{int4, nullable, text, Encoder};

    #[test]
    fn binary_copy_payload_wraps_header_rows_and_trailer() {
        let copy: CopyIn<(i32, Option<String>)> =
            CopyIn::binary("COPY demo FROM STDIN BINARY", (int4, nullable(text)));

        let payload = copy
            .encode_rows([(7_i32, Some("hi".to_string())), (9_i32, None)])
            .unwrap();

        assert_eq!(&payload[0][..11], b"PGCOPY\n\xff\r\n\0");
        assert_eq!(&payload[0][11..], &[0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            payload[1].as_ref(),
            &[
                0, 2, // field count
                0, 0, 0, 4, 0, 0, 0, 7, // int4
                0, 0, 0, 2, b'h', b'i', // text
            ]
        );
        assert_eq!(
            payload[2].as_ref(),
            &[
                0, 2, // field count
                0, 0, 0, 4, 0, 0, 0, 9, // int4
                255, 255, 255, 255, // NULL
            ]
        );
        assert_eq!(payload[3].as_ref(), &[255, 255]);
    }

    #[test]
    fn copy_rejects_text_only_param_codecs() {
        struct TextOnly;

        impl Encoder<i32> for TextOnly {
            fn encode(&self, value: &i32, params: &mut Vec<Option<Vec<u8>>>) -> Result<()> {
                params.push(Some(value.to_string().into_bytes()));
                Ok(())
            }

            fn oids(&self) -> &'static [Oid] {
                &[crate::types::INT4]
            }
        }

        let copy: CopyIn<i32> = CopyIn::binary("COPY demo FROM STDIN BINARY", TextOnly);
        let err = copy.encode_rows([1_i32]).unwrap_err();
        match err {
            Error::Codec(message) => assert!(message.contains("requires binary row codecs")),
            other => panic!("expected codec error, got {other:?}"),
        }
    }
}
