//! M5 integration coverage for optional codecs and `#[derive(Codec)]`.

mod common;

use babar::query::{Command, Query};
#[cfg(feature = "net")]
use std::net::{IpAddr, Ipv4Addr};

use babar::{types, Session};
use common::{AuthMode, PgContainer};

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
