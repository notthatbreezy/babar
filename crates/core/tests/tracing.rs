//! Tracing integration tests.

mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use babar::codec::int4;
use babar::query::{Command, Query};
use babar::{Pool, PoolConfig, Session};
use common::{AuthMode, PgContainer};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Registry;

fn require_docker() -> bool {
    let ok = std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!("skipping: docker unavailable");
    }
    ok
}

#[derive(Debug, Clone)]
struct RecordedSpan {
    name: String,
    fields: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
struct CaptureLayer {
    spans: Arc<Mutex<Vec<RecordedSpan>>>,
}

impl<S> Layer<S> for CaptureLayer
where
    S: tracing::Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        attrs.record(&mut visitor);
        if let Some(span) = ctx.span(id) {
            self.spans.lock().unwrap().push(RecordedSpan {
                name: span.metadata().name().to_string(),
                fields: visitor.fields,
            });
        }
    }
}

#[derive(Default)]
struct FieldVisitor {
    fields: BTreeMap<String, String>,
}

impl Visit for FieldVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

#[tokio::test]
async fn session_emits_expected_tracing_spans() {
    if !require_docker() {
        return;
    }

    let capture = CaptureLayer::default();
    let subscriber = Registry::default().with(capture.clone());
    let _guard = tracing::subscriber::set_default(subscriber);

    let pg = PgContainer::start(AuthMode::Scram).await;
    let session = Session::connect(pg.config(pg.user(), pg.password()))
        .await
        .expect("connect");

    let q: Query<(i32,), (i32,)> = Query::raw("SELECT $1::int4 + 1", (int4,), (int4,));
    let cmd: Command<()> = Command::raw("CREATE TEMP TABLE tracing_demo (id int4)", ());

    session.execute(&cmd, ()).await.expect("execute");
    session.prepare_query(&q).await.expect("prepare");
    session
        .transaction(traced_transaction_body)
        .await
        .expect("transaction");
    session.close().await.expect("close");

    let spans = capture.spans.lock().unwrap().clone();
    assert!(spans.iter().any(|span| span.name == "db.connect"));
    assert!(spans
        .iter()
        .any(|span| span.name == "db.prepare" && span.fields.contains_key("db.system")));
    assert!(spans.iter().any(|span| span.name == "db.execute"
        && span.fields.get("db.statement") == Some(&"SELECT $1::int4 + 1".to_string())));
    assert!(spans.iter().any(|span| span.name == "db.transaction"
        && span.fields.get("db.operation") == Some(&"transaction".to_string())));
}

#[tokio::test]
async fn pool_connect_preserves_tracing_on_acquire() {
    if !require_docker() {
        return;
    }

    let capture = CaptureLayer::default();
    let subscriber = Registry::default().with(capture.clone());
    let _guard = tracing::subscriber::set_default(subscriber);

    let pg = PgContainer::start(AuthMode::Scram).await;
    let pool = Pool::new(pg.config(pg.user(), pg.password()), PoolConfig::new())
        .await
        .expect("pool");
    let conn = pool.acquire().await.expect("acquire");
    let q: Query<(i32,), (i32,)> = Query::raw("SELECT $1::int4", (int4,), (int4,));
    let rows = conn.query(&q, (7,)).await.expect("query");
    assert_eq!(rows, vec![(7,)]);
    drop(conn);
    pool.close().await;

    let spans = capture.spans.lock().unwrap().clone();
    assert!(spans.iter().any(|span| span.name == "db.connect"));
    assert!(spans.iter().any(|span| span.name == "db.execute"));
}

async fn traced_transaction_body(tx: babar::Transaction<'_>) -> babar::Result<()> {
    let q: Query<(i32,), (i32,)> = Query::raw("SELECT $1::int4 + 1", (int4,), (int4,));
    let rows = tx.query(&q, (41,)).await?;
    assert_eq!(rows, vec![(42,)]);
    Ok(())
}
