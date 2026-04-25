//! Postgres type metadata.
//!
//! For M1 we just need the OID constants for the primitive scalar types
//! the text-format codecs cover. M5 will broaden this when we add
//! `uuid`/`time`/`json`/etc.

/// Postgres type OID — a 32-bit identifier the server uses for every type
/// in `pg_type`. The driver uses these to validate decoder shape against
/// `RowDescription` at execute time (M1) and prepare time (M2).
pub type Oid = u32;

/// `bool` (`pg_type.typname = 'bool'`).
pub const BOOL: Oid = 16;

/// `bytea`.
pub const BYTEA: Oid = 17;

/// `bpchar` (blank-padded char(N)).
pub const BPCHAR: Oid = 1042;

/// `varchar` (varying char(N)).
pub const VARCHAR: Oid = 1043;

/// `text`.
pub const TEXT: Oid = 25;

/// `int2` / `smallint` / `i16`.
pub const INT2: Oid = 21;

/// `int4` / `int` / `i32`.
pub const INT4: Oid = 23;

/// `int8` / `bigint` / `i64`.
pub const INT8: Oid = 20;

/// `float4` / `real` / `f32`.
pub const FLOAT4: Oid = 700;

/// `float8` / `double precision` / `f64`.
pub const FLOAT8: Oid = 701;
