//! Postgres type metadata.
//!
//! The driver uses OIDs to validate prepared-statement schemas against the
//! codecs the caller selected. M5 broadens the set beyond the M1 primitives so
//! optional codec modules can expose accurate metadata while remaining fully
//! feature-gated.

/// Postgres type OID — a 32-bit identifier the server uses for every type in
/// `pg_type`.
pub type Oid = u32;

/// Declarative type metadata for one Postgres slot.
///
/// Most built-in Postgres types have globally stable OIDs, so codecs can name
/// them directly. Extension-defined types such as PostGIS `geometry` do not, so
/// they are described by stable SQL type name plus extension name and resolved
/// per-session before preparing a statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Type {
    oid: Oid,
    name: &'static str,
    extension: Option<&'static str>,
}

impl Type {
    /// Build metadata for a built-in type with a globally stable OID.
    pub const fn fixed(oid: Oid, name: &'static str) -> Self {
        Self {
            oid,
            name,
            extension: None,
        }
    }

    /// Build metadata for an extension-defined type whose OID is resolved per
    /// session from the owning extension schema.
    pub const fn extension(name: &'static str, extension: &'static str) -> Self {
        Self {
            oid: 0,
            name,
            extension: Some(extension),
        }
    }

    /// Build metadata for a type that should be resolved by SQL name alone.
    pub const fn unresolved(name: &'static str) -> Self {
        Self {
            oid: 0,
            name,
            extension: None,
        }
    }

    /// The declared OID for this type, or `0` when it must be resolved from
    /// the server at runtime.
    pub const fn oid(self) -> Oid {
        self.oid
    }

    /// SQL type name, for example `int4` or `geometry`.
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// Owning extension name for dynamic types, for example `postgis`.
    pub const fn extension_name(self) -> Option<&'static str> {
        self.extension
    }

    /// Whether this type already carries a concrete OID.
    pub const fn is_resolved(self) -> bool {
        self.oid != 0
    }
}

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
/// `macaddr`.
pub const MACADDR: Oid = 829;
/// `macaddr8`.
pub const MACADDR8: Oid = 774;
/// `bit`.
pub const BIT: Oid = 1560;
/// `varbit`.
pub const VARBIT: Oid = 1562;
/// `tsvector`.
pub const TSVECTOR: Oid = 3614;
/// `tsquery`.
pub const TSQUERY: Oid = 3615;

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

/// `int4multirange`.
pub const INT4_MULTIRANGE: Oid = 4451;
/// `nummultirange`.
pub const NUM_MULTIRANGE: Oid = 4532;
/// `tsmultirange`.
pub const TS_MULTIRANGE: Oid = 4533;
/// `tstzmultirange`.
pub const TSTZ_MULTIRANGE: Oid = 4534;
/// `datemultirange`.
pub const DATE_MULTIRANGE: Oid = 4535;
/// `int8multirange`.
pub const INT8_MULTIRANGE: Oid = 4536;

/// Dynamic PostGIS `geometry`.
pub const GEOMETRY_TYPE: Type = Type::extension("geometry", "postgis");
/// Dynamic PostGIS `geography`.
pub const GEOGRAPHY_TYPE: Type = Type::extension("geography", "postgis");
/// Dynamic `pgvector` `vector`.
pub const VECTOR_TYPE: Type = Type::extension("vector", "vector");
/// Dynamic `hstore`.
pub const HSTORE_TYPE: Type = Type::extension("hstore", "hstore");
/// Dynamic `citext`.
pub const CITEXT_TYPE: Type = Type::extension("citext", "citext");
/// Built-in `tsvector`.
pub const TSVECTOR_TYPE: Type = Type::fixed(TSVECTOR, "tsvector");
/// Built-in `tsquery`.
pub const TSQUERY_TYPE: Type = Type::fixed(TSQUERY, "tsquery");
/// Built-in `macaddr`.
pub const MACADDR_TYPE: Type = Type::fixed(MACADDR, "macaddr");
/// Built-in `macaddr8`.
pub const MACADDR8_TYPE: Type = Type::fixed(MACADDR8, "macaddr8");
/// Built-in `bit`.
pub const BIT_TYPE: Type = Type::fixed(BIT, "bit");
/// Built-in `varbit`.
pub const VARBIT_TYPE: Type = Type::fixed(VARBIT, "varbit");
/// Built-in `int4multirange`.
pub const INT4_MULTIRANGE_TYPE: Type = Type::fixed(INT4_MULTIRANGE, "int4multirange");
/// Built-in `nummultirange`.
pub const NUM_MULTIRANGE_TYPE: Type = Type::fixed(NUM_MULTIRANGE, "nummultirange");
/// Built-in `tsmultirange`.
pub const TS_MULTIRANGE_TYPE: Type = Type::fixed(TS_MULTIRANGE, "tsmultirange");
/// Built-in `tstzmultirange`.
pub const TSTZ_MULTIRANGE_TYPE: Type = Type::fixed(TSTZ_MULTIRANGE, "tstzmultirange");
/// Built-in `datemultirange`.
pub const DATE_MULTIRANGE_TYPE: Type = Type::fixed(DATE_MULTIRANGE, "datemultirange");
/// Built-in `int8multirange`.
pub const INT8_MULTIRANGE_TYPE: Type = Type::fixed(INT8_MULTIRANGE, "int8multirange");

/// Map known built-in OIDs to richer [`Type`] metadata.
pub const fn known_type_for_oid(oid: Oid) -> Option<Type> {
    Some(match oid {
        BOOL => Type::fixed(BOOL, "bool"),
        BYTEA => Type::fixed(BYTEA, "bytea"),
        BPCHAR => Type::fixed(BPCHAR, "bpchar"),
        VARCHAR => Type::fixed(VARCHAR, "varchar"),
        TEXT => Type::fixed(TEXT, "text"),
        INT2 => Type::fixed(INT2, "int2"),
        INT4 => Type::fixed(INT4, "int4"),
        INT8 => Type::fixed(INT8, "int8"),
        FLOAT4 => Type::fixed(FLOAT4, "float4"),
        FLOAT8 => Type::fixed(FLOAT8, "float8"),
        UUID => Type::fixed(UUID, "uuid"),
        DATE => Type::fixed(DATE, "date"),
        TIME => Type::fixed(TIME, "time"),
        TIMESTAMP => Type::fixed(TIMESTAMP, "timestamp"),
        TIMESTAMPTZ => Type::fixed(TIMESTAMPTZ, "timestamptz"),
        JSON => Type::fixed(JSON, "json"),
        JSONB => Type::fixed(JSONB, "jsonb"),
        NUMERIC => Type::fixed(NUMERIC, "numeric"),
        INET => Type::fixed(INET, "inet"),
        CIDR => Type::fixed(CIDR, "cidr"),
        INTERVAL => Type::fixed(INTERVAL, "interval"),
        MACADDR => MACADDR_TYPE,
        MACADDR8 => MACADDR8_TYPE,
        BIT => BIT_TYPE,
        VARBIT => VARBIT_TYPE,
        TSVECTOR => TSVECTOR_TYPE,
        TSQUERY => TSQUERY_TYPE,
        BOOL_ARRAY => Type::fixed(BOOL_ARRAY, "_bool"),
        BYTEA_ARRAY => Type::fixed(BYTEA_ARRAY, "_bytea"),
        INT2_ARRAY => Type::fixed(INT2_ARRAY, "_int2"),
        INT4_ARRAY => Type::fixed(INT4_ARRAY, "_int4"),
        TEXT_ARRAY => Type::fixed(TEXT_ARRAY, "_text"),
        INT8_ARRAY => Type::fixed(INT8_ARRAY, "_int8"),
        FLOAT4_ARRAY => Type::fixed(FLOAT4_ARRAY, "_float4"),
        FLOAT8_ARRAY => Type::fixed(FLOAT8_ARRAY, "_float8"),
        VARCHAR_ARRAY => Type::fixed(VARCHAR_ARRAY, "_varchar"),
        BPCHAR_ARRAY => Type::fixed(BPCHAR_ARRAY, "_bpchar"),
        INET_ARRAY => Type::fixed(INET_ARRAY, "_inet"),
        CIDR_ARRAY => Type::fixed(CIDR_ARRAY, "_cidr"),
        DATE_ARRAY => Type::fixed(DATE_ARRAY, "_date"),
        TIME_ARRAY => Type::fixed(TIME_ARRAY, "_time"),
        TIMESTAMP_ARRAY => Type::fixed(TIMESTAMP_ARRAY, "_timestamp"),
        TIMESTAMPTZ_ARRAY => Type::fixed(TIMESTAMPTZ_ARRAY, "_timestamptz"),
        INTERVAL_ARRAY => Type::fixed(INTERVAL_ARRAY, "_interval"),
        NUMERIC_ARRAY => Type::fixed(NUMERIC_ARRAY, "_numeric"),
        UUID_ARRAY => Type::fixed(UUID_ARRAY, "_uuid"),
        JSON_ARRAY => Type::fixed(JSON_ARRAY, "_json"),
        JSONB_ARRAY => Type::fixed(JSONB_ARRAY, "_jsonb"),
        INT4_RANGE => Type::fixed(INT4_RANGE, "int4range"),
        NUM_RANGE => Type::fixed(NUM_RANGE, "numrange"),
        TS_RANGE => Type::fixed(TS_RANGE, "tsrange"),
        TSTZ_RANGE => Type::fixed(TSTZ_RANGE, "tstzrange"),
        DATE_RANGE => Type::fixed(DATE_RANGE, "daterange"),
        INT8_RANGE => Type::fixed(INT8_RANGE, "int8range"),
        INT4_MULTIRANGE => INT4_MULTIRANGE_TYPE,
        NUM_MULTIRANGE => NUM_MULTIRANGE_TYPE,
        TS_MULTIRANGE => TS_MULTIRANGE_TYPE,
        TSTZ_MULTIRANGE => TSTZ_MULTIRANGE_TYPE,
        DATE_MULTIRANGE => DATE_MULTIRANGE_TYPE,
        INT8_MULTIRANGE => INT8_MULTIRANGE_TYPE,
        _ => return None,
    })
}

/// Convert an OID slice into richer [`Type`] metadata.
pub fn types_for_oids(oids: &[Oid]) -> &'static [Type] {
    if oids.is_empty() {
        return &[];
    }

    let mut all = Vec::with_capacity(oids.len());
    for &oid in oids {
        all.push(known_type_for_oid(oid).unwrap_or(Type::fixed(oid, "")));
    }
    Box::leak(all.into_boxed_slice())
}
