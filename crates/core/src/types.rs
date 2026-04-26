//! Postgres type metadata.
//!
//! The driver uses OIDs to validate prepared-statement schemas against the
//! codecs the caller selected. M5 broadens the set beyond the M1 primitives so
//! optional codec modules can expose accurate metadata while remaining fully
//! feature-gated.

/// Postgres type OID — a 32-bit identifier the server uses for every type in
/// `pg_type`.
pub type Oid = u32;

/// `bool` (`pg_type.typname = 'bool'`).
pub const BOOL: Oid = 16;
/// `bytea`.
pub const BYTEA: Oid = 17;
/// `bpchar` (blank-padded `char(n)`).
pub const BPCHAR: Oid = 1042;
/// `varchar`.
pub const VARCHAR: Oid = 1043;
/// `text`.
pub const TEXT: Oid = 25;
/// `int2` / `smallint` / `i16`.
pub const INT2: Oid = 21;
/// `int4` / `integer` / `i32`.
pub const INT4: Oid = 23;
/// `int8` / `bigint` / `i64`.
pub const INT8: Oid = 20;
/// `float4` / `real` / `f32`.
pub const FLOAT4: Oid = 700;
/// `float8` / `double precision` / `f64`.
pub const FLOAT8: Oid = 701;
/// `uuid`.
pub const UUID: Oid = 2950;
/// `date`.
pub const DATE: Oid = 1082;
/// `time`.
pub const TIME: Oid = 1083;
/// `timestamp`.
pub const TIMESTAMP: Oid = 1114;
/// `timestamptz`.
pub const TIMESTAMPTZ: Oid = 1184;
/// `json`.
pub const JSON: Oid = 114;
/// `jsonb`.
pub const JSONB: Oid = 3802;
/// `numeric`.
pub const NUMERIC: Oid = 1700;
/// `inet`.
pub const INET: Oid = 869;
/// `cidr`.
pub const CIDR: Oid = 650;
/// `interval`.
pub const INTERVAL: Oid = 1186;

/// `bool[]`.
pub const BOOL_ARRAY: Oid = 1000;
/// `bytea[]`.
pub const BYTEA_ARRAY: Oid = 1001;
/// `int2[]`.
pub const INT2_ARRAY: Oid = 1005;
/// `int4[]`.
pub const INT4_ARRAY: Oid = 1007;
/// `text[]`.
pub const TEXT_ARRAY: Oid = 1009;
/// `int8[]`.
pub const INT8_ARRAY: Oid = 1016;
/// `float4[]`.
pub const FLOAT4_ARRAY: Oid = 1021;
/// `float8[]`.
pub const FLOAT8_ARRAY: Oid = 1022;
/// `varchar[]`.
pub const VARCHAR_ARRAY: Oid = 1015;
/// `bpchar[]`.
pub const BPCHAR_ARRAY: Oid = 1014;
/// `inet[]`.
pub const INET_ARRAY: Oid = 1041;
/// `cidr[]`.
pub const CIDR_ARRAY: Oid = 651;
/// `date[]`.
pub const DATE_ARRAY: Oid = 1182;
/// `time[]`.
pub const TIME_ARRAY: Oid = 1183;
/// `timestamp[]`.
pub const TIMESTAMP_ARRAY: Oid = 1115;
/// `timestamptz[]`.
pub const TIMESTAMPTZ_ARRAY: Oid = 1185;
/// `interval[]`.
pub const INTERVAL_ARRAY: Oid = 1187;
/// `numeric[]`.
pub const NUMERIC_ARRAY: Oid = 1231;
/// `uuid[]`.
pub const UUID_ARRAY: Oid = 2951;
/// `json[]`.
pub const JSON_ARRAY: Oid = 199;
/// `jsonb[]`.
pub const JSONB_ARRAY: Oid = 3807;

/// `int4range`.
pub const INT4_RANGE: Oid = 3904;
/// `numrange`.
pub const NUM_RANGE: Oid = 3906;
/// `tsrange`.
pub const TS_RANGE: Oid = 3908;
/// `tstzrange`.
pub const TSTZ_RANGE: Oid = 3910;
/// `daterange`.
pub const DATE_RANGE: Oid = 3912;
/// `int8range`.
pub const INT8_RANGE: Oid = 3926;
