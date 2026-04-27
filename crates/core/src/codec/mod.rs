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
//! [`Encoder::format_codes`] and [`Decoder::format_codes`]. The driver
//! negotiates binary format when both the encoder and decoder support it.
//! Postgres format codes: `0` = text, `1` = binary.

#![allow(non_upper_case_globals)]

#[cfg(feature = "array")]
mod array;
#[cfg(feature = "chrono")]
mod chrono;
#[cfg(feature = "interval")]
mod interval;
#[cfg(feature = "json")]
mod json;
#[cfg(feature = "net")]
mod net;
mod nullable;
#[cfg(feature = "numeric")]
mod numeric;
#[cfg(feature = "postgis")]
mod postgis;
mod primitive;
#[cfg(test)]
mod proptests;
#[cfg(feature = "range")]
mod range;
#[cfg(feature = "time")]
mod time;
mod tuple;
#[cfg(feature = "uuid")]
mod uuid;

#[cfg(feature = "array")]
pub use array::{array, Array, ArrayCodec, ArrayDimension};
#[cfg(feature = "chrono")]
pub use chrono::{
    chrono_date, chrono_time, chrono_timestamp, chrono_timestamptz, ChronoDateCodec,
    ChronoDateTimeCodec, ChronoTimeCodec, ChronoTimestampCodec,
};
#[cfg(feature = "interval")]
pub use interval::{interval, Interval, IntervalCodec};
#[cfg(feature = "json")]
pub use json::{
    json, jsonb, typed_json, typed_json_text, JsonCodec, JsonbCodec, TypedJsonCodec,
    TypedJsonTextCodec,
};
#[cfg(feature = "net")]
pub use net::{cidr, inet, CidrCodec, InetCodec};
pub use nullable::{nullable, Nullable};
#[cfg(feature = "numeric")]
pub use numeric::{numeric, NumericCodec};
#[cfg(feature = "postgis")]
#[cfg_attr(docsrs, doc(cfg(feature = "postgis")))]
pub use postgis::{Geography, Geometry, SpatialKind, Srid};
pub use primitive::{
    bool, bpchar, bytea, float4, float8, int2, int4, int8, text, varchar, BoolCodec, BpcharCodec,
    ByteaCodec, Float4Codec, Float8Codec, Int2Codec, Int4Codec, Int8Codec, TextCodec, VarcharCodec,
};
#[cfg(feature = "range")]
pub use range::{range, Range, RangeBound, RangeCodec};
#[cfg(feature = "time")]
pub use time::{
    date, time, timestamp, timestamptz, DateCodec, OffsetDateTimeCodec, PrimitiveDateTimeCodec,
    TimeCodec,
};
#[cfg(feature = "uuid")]
pub use uuid::{uuid, UuidCodec};

use bytes::Bytes;

use crate::error::Result;
use crate::types::{self, Oid, Type};

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
    ///
    /// Dynamic extension codecs return `0` for slots whose concrete OID is
    /// resolved from [`Encoder::types`] when the statement is prepared.
    fn oids(&self) -> &'static [Oid];

    /// Richer type metadata for the parameter slots this encoder produces.
    fn types(&self) -> &'static [Type] {
        types::types_for_oids(self.oids())
    }

    /// Postgres format codes for parameter slots, in order. Each element
    /// is `0` (text) or `1` (binary). Default: text for all.
    fn format_codes(&self) -> &'static [i16] {
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
    ///
    /// Dynamic extension codecs return `0` for slots whose concrete OID is
    /// resolved from [`Decoder::types`] when validating a prepared statement.
    fn oids(&self) -> &'static [Oid];

    /// Richer type metadata for the columns this decoder consumes.
    fn types(&self) -> &'static [Type] {
        types::types_for_oids(self.oids())
    }

    /// Postgres format codes for result columns, in order. Each element
    /// is `0` (text) or `1` (binary). Default: text for all.
    fn format_codes(&self) -> &'static [i16] {
        &[]
    }
}

/// A codec is anything that's both an [`Encoder`] and [`Decoder`] for the same
/// type.
pub trait Codec<A>: Encoder<A> + Decoder<A> {}

impl<C, A> Codec<A> for C where C: Encoder<A> + Decoder<A> {}
