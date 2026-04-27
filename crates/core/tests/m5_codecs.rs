//! M5 integration coverage for optional codecs and `#[derive(Codec)]`.

mod common;

use babar::query::{Command, Query};
#[cfg(feature = "hstore")]
use std::collections::BTreeMap;
#[cfg(feature = "net")]
use std::net::{IpAddr, Ipv4Addr};

use babar::{types, Session};
use common::{AuthMode, PgContainer};

#[cfg(feature = "postgis")]
const DEFAULT_POSTGIS_IMAGE: &str = "postgis/postgis:17-3.5-alpine";

fn require_docker() -> bool {
    let ok = std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !ok {
        eprintln!("skipping: docker unavailable");
    }
    ok
}

async fn fresh_session() -> Option<(PgContainer, Session)> {
    if !require_docker() {
        return None;
    }
    let pg = PgContainer::start(AuthMode::Scram).await;
    let session = Session::connect(pg.config(pg.user(), pg.password()))
        .await
        .expect("connect");
    Some((pg, session))
}

#[cfg(feature = "postgis")]
async fn fresh_postgis_session() -> Option<(PgContainer, Session)> {
    if !require_docker() {
        return None;
    }

    let image =
        std::env::var("BABAR_POSTGIS_IMAGE").unwrap_or_else(|_| DEFAULT_POSTGIS_IMAGE.to_string());
    let pg = PgContainer::start_with_image(AuthMode::Scram, image).await;
    let session = Session::connect(pg.config(pg.user(), pg.password()))
        .await
        .expect("connect");
    session
        .simple_query_raw("CREATE EXTENSION IF NOT EXISTS postgis")
        .await
        .expect("create postgis extension");
    Some((pg, session))
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct DerivedRow {
    #[pg(codec = "int4")]
    id: i32,
    #[pg(codec = "text")]
    name: String,
    #[pg(codec = "bool")]
    active: bool,
    #[pg(codec = "nullable(text)")]
    note: Option<String>,
    #[pg(codec = "int8")]
    visits: i64,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct InferredRow {
    id: i32,
    name: String,
    active: bool,
    payload: Vec<u8>,
    note: Option<String>,
    visits: i64,
}

#[derive(Debug, Clone, PartialEq, babar::Codec)]
struct MixedOverrideRow {
    id: i32,
    #[pg(codec = "varchar")]
    label: String,
    note: Option<String>,
    active: bool,
}

type DerivedQuery = Query<(), DerivedRow>;
#[cfg(feature = "time")]
type TimeTuple = (
    ::time::Date,
    ::time::Time,
    ::time::PrimitiveDateTime,
    ::time::OffsetDateTime,
);
#[cfg(feature = "chrono")]
type ChronoTuple = (
    ::chrono::NaiveDate,
    ::chrono::NaiveTime,
    ::chrono::NaiveDateTime,
    ::chrono::DateTime<::chrono::Utc>,
);
#[cfg(feature = "array")]
type IntTextArrays = (babar::codec::Array<i32>, babar::codec::Array<String>);
#[cfg(feature = "postgis")]
type PostgisRow = (
    babar::codec::Geometry<geo_types::Point<f64>>,
    babar::codec::Geometry<geo_types::LineString<f64>>,
    babar::codec::Geometry<geo_types::Polygon<f64>>,
    babar::codec::Geometry<geo_types::MultiPolygon<f64>>,
    babar::codec::Geography<geo_types::Point<f64>>,
);
#[cfg(feature = "multirange")]
type Int4Multiranges = (babar::codec::Multirange<i32>,);

#[test]
fn derive_codec_uses_inferred_and_override_oids() {
    assert_eq!(
        <_ as babar::codec::Encoder<InferredRow>>::oids(&InferredRow::CODEC),
        &[
            types::INT4,
            types::TEXT,
            types::BOOL,
            types::BYTEA,
            types::TEXT,
            types::INT8,
        ]
    );
    assert_eq!(
        <_ as babar::codec::Decoder<InferredRow>>::oids(&InferredRow::CODEC),
        &[
            types::INT4,
            types::TEXT,
            types::BOOL,
            types::BYTEA,
            types::TEXT,
            types::INT8,
        ]
    );
    assert_eq!(
        <_ as babar::codec::Encoder<MixedOverrideRow>>::oids(&MixedOverrideRow::CODEC),
        &[types::INT4, types::VARCHAR, types::TEXT, types::BOOL]
    );
}

#[tokio::test]
async fn derive_codec_roundtrips_struct_rows() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    session
        .simple_query_raw(
            "CREATE TEMP TABLE derive_rows (\
                id int4 PRIMARY KEY,\
                name text NOT NULL,\
                active bool NOT NULL,\
                note text,\
                visits int8 NOT NULL\
            )",
        )
        .await
        .expect("create table");

    let insert: Command<DerivedRow> = Command::raw(
        "INSERT INTO derive_rows (id, name, active, note, visits) VALUES ($1, $2, $3, $4, $5)",
        DerivedRow::CODEC,
    );
    let expected = vec![
        DerivedRow {
            id: 1,
            name: "alice".into(),
            active: true,
            note: Some("first".into()),
            visits: 4,
        },
        DerivedRow {
            id: 2,
            name: "bob".into(),
            active: false,
            note: None,
            visits: 9,
        },
    ];
    for row in &expected {
        let affected = session
            .execute(&insert, row.clone())
            .await
            .expect("insert row");
        assert_eq!(affected, 1);
    }

    let select: DerivedQuery = Query::raw(
        "SELECT id, name, active, note, visits FROM derive_rows ORDER BY id",
        (),
        DerivedRow::CODEC,
    );
    let actual = session.query(&select, ()).await.expect("select rows");
    assert_eq!(actual, expected);

    session.close().await.expect("close");
}

#[tokio::test]
async fn derive_codec_roundtrips_inferred_rows() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    session
        .simple_query_raw(
            "CREATE TEMP TABLE inferred_rows (\
                id int4 PRIMARY KEY,\
                name text NOT NULL,\
                active bool NOT NULL,\
                payload bytea NOT NULL,\
                note text,\
                visits int8 NOT NULL\
            )",
        )
        .await
        .expect("create table");

    let insert: Command<InferredRow> = Command::raw(
        "INSERT INTO inferred_rows (id, name, active, payload, note, visits) VALUES ($1, $2, $3, $4, $5, $6)",
        InferredRow::CODEC,
    );
    let expected = vec![
        InferredRow {
            id: 1,
            name: "alice".into(),
            active: true,
            payload: b"alpha".to_vec(),
            note: Some("first".into()),
            visits: 4,
        },
        InferredRow {
            id: 2,
            name: "bob".into(),
            active: false,
            payload: b"beta".to_vec(),
            note: None,
            visits: 9,
        },
    ];
    for row in &expected {
        let affected = session
            .execute(&insert, row.clone())
            .await
            .expect("insert row");
        assert_eq!(affected, 1);
    }

    let select: Query<(), InferredRow> = Query::raw(
        "SELECT id, name, active, payload, note, visits FROM inferred_rows ORDER BY id",
        (),
        InferredRow::CODEC,
    );
    let actual = session.query(&select, ()).await.expect("select rows");
    assert_eq!(actual, expected);

    session.close().await.expect("close");
}

#[tokio::test]
async fn derive_codec_roundtrips_mixed_inferred_and_explicit_rows() {
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    session
        .simple_query_raw(
            "CREATE TEMP TABLE mixed_override_rows (\
                id int4 PRIMARY KEY,\
                label varchar(64) NOT NULL,\
                note text,\
                active bool NOT NULL\
            )",
        )
        .await
        .expect("create table");

    let insert: Command<MixedOverrideRow> = Command::raw(
        "INSERT INTO mixed_override_rows (id, label, note, active) VALUES ($1, $2, $3, $4)",
        MixedOverrideRow::CODEC,
    );
    let expected = vec![
        MixedOverrideRow {
            id: 1,
            label: "alpha".into(),
            note: Some("first".into()),
            active: true,
        },
        MixedOverrideRow {
            id: 2,
            label: "beta".into(),
            note: None,
            active: false,
        },
    ];
    for row in &expected {
        let affected = session
            .execute(&insert, row.clone())
            .await
            .expect("insert row");
        assert_eq!(affected, 1);
    }

    let select: Query<(), MixedOverrideRow> = Query::raw(
        "SELECT id, label, note, active FROM mixed_override_rows ORDER BY id",
        (),
        MixedOverrideRow::CODEC,
    );
    let actual = session.query(&select, ()).await.expect("select rows");
    assert_eq!(actual, expected);

    session.close().await.expect("close");
}

#[cfg(feature = "uuid")]
#[tokio::test]
async fn uuid_codec_roundtrip() {
    use babar::codec::uuid;
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let value = "550e8400-e29b-41d4-a716-446655440000"
        .parse::<::uuid::Uuid>()
        .unwrap();
    let query: Query<(::uuid::Uuid,), (::uuid::Uuid,)> =
        Query::raw("SELECT $1::uuid", (uuid,), (uuid,));
    let rows = session.query(&query, (value,)).await.expect("select uuid");
    assert_eq!(rows, vec![(value,)]);
    session.close().await.expect("close");
}

#[cfg(feature = "macaddr")]
#[tokio::test]
async fn macaddr_codecs_roundtrip() {
    use babar::codec::{macaddr, macaddr8, MacAddr, MacAddr8};

    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let mac = MacAddr::from([0x08, 0x00, 0x2b, 0x01, 0x02, 0x03]);
    let mac8 = MacAddr8::from([0x08, 0x00, 0x2b, 0xff, 0xfe, 0x01, 0x02, 0x03]);
    let query: Query<(MacAddr, MacAddr8), (MacAddr, MacAddr8)> = Query::raw(
        "SELECT $1::macaddr, $2::macaddr8",
        (macaddr, macaddr8),
        (macaddr, macaddr8),
    );
    let rows = session
        .query(&query, (mac, mac8))
        .await
        .expect("select macaddr values");
    assert_eq!(rows, vec![(mac, mac8)]);

    session.close().await.expect("close");
}

#[cfg(feature = "bits")]
#[tokio::test]
async fn bit_codecs_roundtrip() {
    use babar::codec::{bit, varbit, BitString};

    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let fixed = BitString::from_text("10110010").expect("fixed bit string");
    let varying = BitString::from_text("10110").expect("varying bit string");
    let query: Query<(BitString, BitString), (BitString, BitString)> = Query::raw(
        "SELECT $1::bit(8), $2::varbit",
        (bit, varbit),
        (bit, varbit),
    );
    let rows = session
        .query(&query, (fixed.clone(), varying.clone()))
        .await
        .expect("select bit values");
    assert_eq!(rows, vec![(fixed, varying)]);

    session.close().await.expect("close");
}

#[cfg(feature = "citext")]
#[tokio::test]
async fn citext_codec_roundtrip_with_dynamic_type_resolution() {
    use babar::codec::{citext, Decoder, Encoder};

    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    session
        .simple_query_raw("CREATE EXTENSION IF NOT EXISTS citext")
        .await
        .expect("create citext extension");

    let value = "MiXeD".to_string();
    let query: Query<(String,), (String,)> = Query::raw("SELECT $1::citext", (citext,), (citext,));
    let rows = session
        .query(&query, (value.clone(),))
        .await
        .expect("select citext value");
    assert_eq!(rows, vec![(value,)]);
    assert_eq!(Encoder::<String>::oids(&citext), &[0]);
    assert_eq!(Decoder::<String>::oids(&citext), &[0]);
    assert_eq!(query.param_types(), &[types::CITEXT_TYPE]);
    assert_eq!(query.output_types(), &[types::CITEXT_TYPE]);

    session.close().await.expect("close");
}

#[cfg(feature = "hstore")]
#[tokio::test]
async fn hstore_codec_roundtrip_with_stable_map_surface() {
    use babar::codec::{hstore, Decoder, Encoder, Hstore};

    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    session
        .simple_query_raw("CREATE EXTENSION IF NOT EXISTS hstore")
        .await
        .expect("create hstore extension");

    let mut map = BTreeMap::new();
    map.insert("alpha".to_string(), Some("one".to_string()));
    map.insert("beta".to_string(), None);
    map.insert("quoted".to_string(), Some("a\"b".to_string()));
    let value = Hstore::from(map);

    let query: Query<(Hstore,), (Hstore,)> = Query::raw("SELECT $1::hstore", (hstore,), (hstore,));
    let rows = session
        .query(&query, (value.clone(),))
        .await
        .expect("select hstore value");
    assert_eq!(rows, vec![(value,)]);
    assert_eq!(Encoder::<Hstore>::oids(&hstore), &[0]);
    assert_eq!(Decoder::<Hstore>::oids(&hstore), &[0]);
    assert_eq!(query.param_types(), &[types::HSTORE_TYPE]);
    assert_eq!(query.output_types(), &[types::HSTORE_TYPE]);

    session.close().await.expect("close");
}

#[cfg(feature = "time")]
#[tokio::test]
async fn time_codecs_roundtrip() {
    use babar::codec::{
        date as time_date, time as time_time, timestamp as time_timestamp,
        timestamptz as time_timestamptz,
    };
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let day = ::time::Date::from_calendar_date(2024, ::time::Month::March, 14).unwrap();
    let clock = ::time::Time::from_hms_micro(9, 26, 53, 123_456).unwrap();
    let ts = day.with_time(clock);
    let tsz = ts.assume_utc();
    let query: Query<TimeTuple, TimeTuple> = Query::raw(
        "SELECT $1::date, $2::time, $3::timestamp, $4::timestamptz",
        (time_date, time_time, time_timestamp, time_timestamptz),
        (time_date, time_time, time_timestamp, time_timestamptz),
    );
    let rows = session
        .query(&query, (day, clock, ts, tsz))
        .await
        .expect("select temporal row");
    assert_eq!(rows, vec![(day, clock, ts, tsz)]);
    session.close().await.expect("close");
}

#[cfg(feature = "chrono")]
#[tokio::test]
async fn chrono_codecs_roundtrip() {
    use babar::codec::{chrono_date, chrono_time, chrono_timestamp, chrono_timestamptz};
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let day = ::chrono::NaiveDate::from_ymd_opt(2024, 3, 14).unwrap();
    let clock = ::chrono::NaiveTime::from_hms_micro_opt(9, 26, 53, 123_456).unwrap();
    let ts = day.and_time(clock);
    let tsz = ::chrono::DateTime::from_naive_utc_and_offset(ts, ::chrono::Utc);
    let query: Query<ChronoTuple, ChronoTuple> = Query::raw(
        "SELECT $1::date, $2::time, $3::timestamp, $4::timestamptz",
        (
            chrono_date,
            chrono_time,
            chrono_timestamp,
            chrono_timestamptz,
        ),
        (
            chrono_date,
            chrono_time,
            chrono_timestamp,
            chrono_timestamptz,
        ),
    );
    let rows = session
        .query(&query, (day, clock, ts, tsz))
        .await
        .expect("select temporal row");
    assert_eq!(rows, vec![(day, clock, ts, tsz)]);
    session.close().await.expect("close");
}

#[cfg(feature = "json")]
#[tokio::test]
async fn json_codecs_roundtrip() {
    use babar::codec::{json, jsonb, typed_json};
    use serde::{Deserialize, Serialize};
    use serde_json::json as json_value;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Payload {
        id: i32,
        tags: Vec<String>,
    }

    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let raw = json_value!({"ok": true, "count": 3});
    let typed = Payload {
        id: 7,
        tags: vec!["a".into(), "b".into()],
    };
    let query: Query<(serde_json::Value, Payload), (serde_json::Value, Payload)> = Query::raw(
        "SELECT $1::json, $2::jsonb",
        (json, typed_json::<Payload>()),
        (json, typed_json::<Payload>()),
    );
    let rows = session
        .query(&query, (raw.clone(), typed.clone()))
        .await
        .expect("select json row");
    assert_eq!(rows, vec![(raw, typed)]);

    let query_b: Query<(serde_json::Value,), (serde_json::Value,)> =
        Query::raw("SELECT $1::jsonb", (jsonb,), (jsonb,));
    let value = json_value!({"typed": false});
    let rows = session
        .query(&query_b, (value.clone(),))
        .await
        .expect("select jsonb row");
    assert_eq!(rows, vec![(value,)]);
    session.close().await.expect("close");
}

#[cfg(feature = "numeric")]
#[tokio::test]
async fn numeric_codec_roundtrip() {
    use babar::codec::numeric;
    use rust_decimal::Decimal;
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let value = Decimal::from_i128_with_scale(123_456_789, 4);
    let query: Query<(Decimal,), (Decimal,)> =
        Query::raw("SELECT $1::numeric", (numeric,), (numeric,));
    let rows = session
        .query(&query, (value,))
        .await
        .expect("select numeric");
    assert_eq!(rows, vec![(value,)]);
    session.close().await.expect("close");
}

#[cfg(feature = "net")]
#[tokio::test]
async fn net_codecs_roundtrip() {
    use babar::codec::{cidr, inet};
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let addr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42));
    let query: Query<(IpAddr, IpAddr), (IpAddr, IpAddr)> =
        Query::raw("SELECT $1::inet, $2::cidr", (inet, cidr), (inet, cidr));
    let rows = session
        .query(&query, (addr, addr))
        .await
        .expect("select net row");
    assert_eq!(rows, vec![(addr, addr)]);
    session.close().await.expect("close");
}

#[cfg(feature = "interval")]
#[tokio::test]
async fn interval_codec_roundtrip() {
    use babar::codec::{interval, Interval};
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let value = Interval::new(14, 3, 987_654_321);
    let query: Query<(Interval,), (Interval,)> =
        Query::raw("SELECT $1::interval", (interval,), (interval,));
    let rows = session
        .query(&query, (value,))
        .await
        .expect("select interval");
    assert_eq!(rows, vec![(value,)]);
    session.close().await.expect("close");
}

#[cfg(feature = "array")]
#[tokio::test]
async fn array_codec_roundtrip() {
    use babar::codec::{array, int4, text, Array, ArrayDimension};
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let one_d = Array::from_vec((0_i32..1000).collect());
    let grid_values = (0..10_000).map(|i| format!("cell-{i}")).collect();
    let two_d = Array::new(
        vec![ArrayDimension::new(100, 1), ArrayDimension::new(100, 1)],
        grid_values,
    )
    .expect("valid 2d array");

    let query: Query<IntTextArrays, IntTextArrays> = Query::raw(
        "SELECT $1::int4[], $2::text[][]",
        (array(int4), array(text)),
        (array(int4), array(text)),
    );
    let rows = session
        .query(&query, (one_d.clone(), two_d.clone()))
        .await
        .expect("select arrays");
    assert_eq!(rows, vec![(one_d, two_d)]);
    session.close().await.expect("close");
}

#[cfg(feature = "range")]
#[tokio::test]
async fn range_codec_roundtrip() {
    use babar::codec::{int4, range, Range, RangeBound};
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let value = Range::NonEmpty {
        lower: RangeBound::Inclusive(10_i32),
        upper: RangeBound::Exclusive(42_i32),
    };
    let query: Query<(Range<i32>,), (Range<i32>,)> =
        Query::raw("SELECT $1::int4range", (range(int4),), (range(int4),));
    let rows = session
        .query(&query, (value.clone(),))
        .await
        .expect("select range");
    assert_eq!(rows, vec![(value,)]);
    session.close().await.expect("close");
}

#[cfg(feature = "postgis")]
#[tokio::test]
async fn postgis_codecs_roundtrip_common_shapes() {
    use babar::codec::{geography, geometry, Geography, Geometry, Srid};
    use geo_types::{LineString, MultiPolygon, Point, Polygon};

    let Some((_pg, session)) = fresh_postgis_session().await else {
        return;
    };

    let planar = Geometry::with_srid(Point::new(1.25, -3.5), Srid::new(3857));
    let route = Geometry::new(LineString::from(vec![(0.0, 0.0), (2.0, 3.0), (5.0, 8.0)]));
    let area = Geometry::with_srid(
        Polygon::new(
            LineString::from(vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 0.0)]),
            vec![LineString::from(vec![
                (1.0, 1.0),
                (2.0, 1.0),
                (1.0, 2.0),
                (1.0, 1.0),
            ])],
        ),
        Srid::new(4326),
    );
    let regions = Geometry::new(MultiPolygon(vec![Polygon::new(
        LineString::from(vec![(10.0, 10.0), (12.0, 10.0), (12.0, 12.0), (10.0, 10.0)]),
        vec![],
    )]));
    let earth = Geography::wgs84(Point::new(-73.9857, 40.7484));

    let query: Query<PostgisRow, PostgisRow> = Query::raw(
        "SELECT $1::geometry, $2::geometry, $3::geometry, $4::geometry, $5::geography",
        (
            geometry::<Point<f64>>(),
            geometry::<LineString<f64>>(),
            geometry::<Polygon<f64>>(),
            geometry::<MultiPolygon<f64>>(),
            geography::<Point<f64>>(),
        ),
        (
            geometry::<Point<f64>>(),
            geometry::<LineString<f64>>(),
            geometry::<Polygon<f64>>(),
            geometry::<MultiPolygon<f64>>(),
            geography::<Point<f64>>(),
        ),
    );

    let rows = session
        .query(
            &query,
            (
                planar.clone(),
                route.clone(),
                area.clone(),
                regions.clone(),
                earth.clone(),
            ),
        )
        .await
        .expect("select postgis row");
    assert_eq!(rows, vec![(planar, route, area, regions, earth)]);

    session.close().await.expect("close");
}

#[cfg(feature = "postgis")]
#[tokio::test]
async fn postgis_reports_documented_geometry_collection_limit() {
    use babar::codec::{geometry, Geometry};
    use geo_types::Geometry as GeoGeometry;

    let Some((_pg, session)) = fresh_postgis_session().await else {
        return;
    };

    let query: Query<(), Geometry<GeoGeometry<f64>>> = Query::raw(
        "SELECT ST_GeomFromText('GEOMETRYCOLLECTION(POINT(1 2))')::geometry",
        (),
        geometry::<GeoGeometry<f64>>(),
    );
    let error = session
        .query(&query, ())
        .await
        .expect_err("geometry collection should be rejected");
    assert!(error.to_string().contains("GeometryCollection"));

    session.close().await.expect("close");
}

#[cfg(feature = "multirange")]
#[tokio::test]
async fn multirange_codec_roundtrip() {
    use babar::codec::{int4, multirange, Multirange, Range, RangeBound};
    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let value = Multirange::new(vec![
        Range::NonEmpty {
            lower: RangeBound::Inclusive(1_i32),
            upper: RangeBound::Exclusive(5_i32),
        },
        Range::NonEmpty {
            lower: RangeBound::Inclusive(10_i32),
            upper: RangeBound::Exclusive(15_i32),
        },
    ]);
    let query: Query<Int4Multiranges, Int4Multiranges> = Query::raw(
        "SELECT $1::int4multirange",
        (multirange(int4),),
        (multirange(int4),),
    );
    let rows = session
        .query(&query, (value.clone(),))
        .await
        .expect("select multirange");
    assert_eq!(rows, vec![(value,)]);
    session.close().await.expect("close");
}

#[cfg(feature = "pgvector")]
#[tokio::test]
async fn pgvector_codec_roundtrip() {
    use babar::codec::{vector, Vector};

    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    if session
        .simple_query_raw("CREATE EXTENSION IF NOT EXISTS vector")
        .await
        .is_err()
    {
        eprintln!("skipping: pgvector extension unavailable");
        session.close().await.expect("close");
        return;
    }

    let value = Vector::new(vec![1.0, -2.5, 3.25]).expect("vector");
    let query: Query<(Vector,), (Vector,)> = Query::raw("SELECT $1::vector", (vector,), (vector,));
    let prepared = session.prepare_query(&query).await.expect("prepare vector");
    let rows = prepared
        .query((value.clone(),))
        .await
        .expect("select vector");
    assert_eq!(rows, vec![(value,)]);
    prepared.close().await.expect("close prepared");
    session.close().await.expect("close");
}

#[cfg(feature = "text-search")]
#[tokio::test]
async fn text_search_codecs_roundtrip() {
    use babar::codec::{tsquery, tsvector, TsQuery, TsVector};

    let Some((_pg, session)) = fresh_session().await else {
        return;
    };

    let vector = TsVector::from("'fat':1 'rat':2");
    let query = TsQuery::from("fat & rat");
    let roundtrip: Query<(TsVector, TsQuery), (TsVector, TsQuery)> = Query::raw(
        "SELECT $1::tsvector, $2::tsquery",
        (tsvector, tsquery),
        (tsvector, tsquery),
    );
    let rows = session
        .query(&roundtrip, (vector, query))
        .await
        .expect("select text-search row");
    assert_eq!(
        rows,
        vec![(
            TsVector::from("'fat':1 'rat':2"),
            TsQuery::from("'fat' & 'rat'"),
        )]
    );
    session.close().await.expect("close");
}
