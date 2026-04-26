//! Encoder, Decoder, Codec traits and primitive codecs.
//!
//! ## Mental model
//!
//! - `Encoder<A>` turns a value of type `A` into one or more Postgres
//!   parameter slots (the `$1`, `$2`, ... in a query).
//! - `Decoder<A>` turns one or more `RowDescription` columns back into a
//!   value of type `A`.
//! - `Codec<A>` is a value that does both. The blanket impl gives every
//!   `Encoder + Decoder` a `Codec` for free.
//!
//! Codec values are usually unit-typed constants imported by name:
//!
//! ```
//! use babar::codec::{int4, text};
//!
//! // int4: Codec<i32>
//! // text: Codec<String>
//! ```
//!
//! They compose: a tuple of codecs is itself a codec for the tuple of
//! values, up to 16-arity. `nullable(c)` lifts a codec into one that
//! accepts SQL `NULL` as `Option::None`.
//!
//! ## Format
//!
//! Codecs advertise which wire formats they support via
//! [`Encoder::format_code`] and [`Decoder::format_code`]. The driver
//! negotiates binary format when both the encoder and decoder support it.
//! Postgres format codes: `0` = text, `1` = binary.

#![allow(non_upper_case_globals)] // codec consts (`int4`, `text`, ...) are lowercase to match Skunk

mod nullable;
mod primitive;
#[cfg(test)]
mod proptests;
mod tuple;

pub use nullable::{nullable, Nullable};
pub use primitive::{
    bool, bpchar, bytea, float4, float8, int2, int4, int8, text, varchar, BoolCodec, BpcharCodec,
    ByteaCodec, Float4Codec, Float8Codec, Int2Codec, Int4Codec, Int8Codec, TextCodec, VarcharCodec,
};

use bytes::Bytes;

use crate::error::Result;
use crate::types::Oid;

/// Postgres wire format code: `0` = text, `1` = binary.
pub const FORMAT_TEXT: i16 = 0;
/// Postgres wire format code: `0` = text, `1` = binary.
pub const FORMAT_BINARY: i16 = 1;

/// Encode a value into one or more Postgres parameter slots.
///
/// Implementors push exactly `oids().len()` entries onto `params`. Each
/// entry is the parameter's encoded bytes; `None` is SQL `NULL`.
pub trait Encoder<A>: Send + Sync {
    /// Append parameter slots for `value` onto `params`.
    fn encode(&self, value: &A, params: &mut Vec<Option<Vec<u8>>>) -> Result<()>;

    /// OIDs of the parameter slots this encoder produces, in order.
    fn oids(&self) -> &'static [Oid];

    /// Postgres format codes for parameter slots, in order. Each element
    /// is `0` (text) or `1` (binary). Default: text for all.
    fn format_codes(&self) -> &'static [i16] {
        // Default: text for all slots. Codecs that support binary
        // override this.
        &[]
    }
}

/// Decode a value from one or more `RowDescription` columns.
///
/// Implementors must consume exactly `n_columns()` columns from the
/// front of the slice. The driver enforces total alignment by checking
/// the top-level decoder's `n_columns()` against the server's
/// `RowDescription`; nested decoders (tuples, `nullable`) trust their
/// callers to slice correctly.
pub trait Decoder<A>: Send + Sync {
    /// Decode `A` from `columns[..n_columns()]`. The slice must be at
    /// least `n_columns()` long.
    fn decode(&self, columns: &[Option<Bytes>]) -> Result<A>;

    /// Number of columns this decoder consumes.
    fn n_columns(&self) -> usize;

    /// OIDs of consumed columns, in order. `oids().len() == n_columns()`.
    fn oids(&self) -> &'static [Oid];

    /// Postgres format codes for result columns, in order. Each element
    /// is `0` (text) or `1` (binary). Default: text for all.
    fn format_codes(&self) -> &'static [i16] {
        &[]
    }
}

/// A codec is anything that's both an [`Encoder`] and [`Decoder`] for the
/// same type.
pub trait Codec<A>: Encoder<A> + Decoder<A> {}

impl<C, A> Codec<A> for C where C: Encoder<A> + Decoder<A> {}
