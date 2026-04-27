# Codec catalog

> Generated rustdoc: <https://docs.rs/babar/latest/babar/codec/index.html>

> See also: [Book Chapter 10 — Custom codecs](../book/10-custom-codecs.md).

Every codec babar ships, grouped by module. OIDs are the Postgres
type OIDs the codec advertises in `Bind` / `RowDescription`. All
codecs use the binary wire format unless noted.

## `babar::codec` (always on)

| Postgres type | OID | Rust type | Codec value | Module |
|---|---|---|---|---|
| `int2` / `smallint` | 21 | `i16` | `int2` | `primitive` |
| `int4` / `integer` | 23 | `i32` | `int4` | `primitive` |
| `int8` / `bigint` | 20 | `i64` | `int8` | `primitive` |
| `float4` / `real` | 700 | `f32` | `float4` | `primitive` |
| `float8` / `double precision` | 701 | `f64` | `float8` | `primitive` |
| `bool` | 16 | `bool` | `bool` | `primitive` |
| `text` | 25 | `String` | `text` | `primitive` |
| `varchar` | 1043 | `String` | `varchar` | `primitive` |
| `bpchar` / `char(n)` | 1042 | `String` | `bpchar` | `primitive` |
| `bytea` | 17 | `Vec<u8>` | `bytea` | `primitive` |
| any (NULL-aware wrapper) | n/a | `Option<T>` | `nullable(C)` | `nullable` |
| `T[]` | array OID | `Vec<T>` | `array(C)` | `array` (feature `array`) |

Codec constants are lowercase to match Postgres type names — `int4`,
`text`, `bool` shadow the Rust primitives inside `babar::codec`.
That's deliberate; import the constants explicitly
(`use babar::codec::{int4, text};`) and the prim names remain visible
elsewhere.

## Optional types — feature-gated

| Postgres type | OID | Rust type | Codec value | Module | Feature |
|---|---|---|---|---|---|
| `uuid` | 2950 | `uuid::Uuid` | `uuid` | `uuid` | `uuid` |
| `date` | 1082 | `time::Date` | `date` | `time` | `time` |
| `time` | 1083 | `time::Time` | `time` | `time` | `time` |
| `timestamp` | 1114 | `time::PrimitiveDateTime` | `timestamp` | `time` | `time` |
| `timestamptz` | 1184 | `time::OffsetDateTime` | `timestamptz` | `time` | `time` |
| `date` | 1082 | `chrono::NaiveDate` | `chrono_date` | `chrono` | `chrono` |
| `time` | 1083 | `chrono::NaiveTime` | `chrono_time` | `chrono` | `chrono` |
| `timestamp` | 1114 | `chrono::NaiveDateTime` | `chrono_timestamp` | `chrono` | `chrono` |
| `timestamptz` | 1184 | `chrono::DateTime<Utc>` | `chrono_timestamptz` | `chrono` | `chrono` |
| `interval` | 1186 | `babar::codec::Interval` | `interval` | `interval` | `interval` |
| `numeric` | 1700 | `rust_decimal::Decimal` | `numeric` | `numeric` | `numeric` |
| `json` | 114 | `serde_json::Value` / `T: Deserialize` | `json` / `typed_json::<T>()` | `json` | `json` |
| `jsonb` | 3802 | `serde_json::Value` / `T: Deserialize` | `jsonb` / `typed_json::<T>()` | `json` | `json` |
| `inet` | 869 | `std::net::IpAddr` | `inet` | `net` | `net` |
| `cidr` | 650 | `babar::codec::Cidr` | `cidr` | `net` | `net` |
| `macaddr` | 829 | `babar::codec::MacAddr` | `macaddr` | `macaddr` | `macaddr` |
| `macaddr8` | 774 | `babar::codec::MacAddr8` | `macaddr8` | `macaddr` | `macaddr` |
| `bit(n)` | 1560 | `babar::codec::BitString` | `bit` | `bits` | `bits` |
| `varbit` | 1562 | `babar::codec::BitString` | `varbit` | `bits` | `bits` |
| `hstore` | server-assigned | `babar::codec::Hstore` | `hstore` | `hstore` | `hstore` |
| `citext` | server-assigned | `String` | `citext` | `citext` | `citext` |
| `tsvector` | 3614 | `babar::codec::TsVector` | `tsvector` | `text_search` | `text-search` |
| `tsquery` | 3615 | `babar::codec::TsQuery` | `tsquery` | `text_search` | `text-search` |
| `vector` | server-assigned | `babar::codec::Vector` | `vector` | `pgvector` | `pgvector` |
| `geometry` (PostGIS) | server-assigned | `T: geo_types::*` | `geometry::<T>()` | `postgis` | `postgis` |
| `geography` (PostGIS) | server-assigned | `T: geo_types::*` | `geography::<T>()` | `postgis` | `postgis` |
| `range<T>` | range OID | `babar::codec::Range<T>` | `range(C)` | `range` | `range` |
| `multirange<T>` | mr OID | `babar::codec::Multirange<T>` | `multirange(C)` | `multirange` | `multirange` (implies `range`) |

## Composing codecs

Most type-system muscle lives in *combinators*, not new codec
modules:

| Combinator | What it does |
|---|---|
| `nullable(C)` | Adds NULL → `Option<T>` handling. Required for any column that can be NULL. |
| `array(C)` | One-dimensional Postgres arrays as `Vec<T>`. |
| `range(C)` | Postgres ranges over `T`. |
| `multirange(C)` | Postgres multiranges (Postgres 14+). |
| `(C1, C2, …)` | A row tuple — `Decoder<(A, B, …)>` is auto-implemented for tuples of decoders. |

For non-`'static` user types, write your own
`Encoder<A>` / `Decoder<A>` (Chapter 10) — the codec module's
`Encoder<UnitStruct>` glue is small.

## Next

For the cargo features that gate these codecs, see
[feature-flags.md](./feature-flags.md). For the error variants codecs
return on bad bytes, see [errors.md](./errors.md).
