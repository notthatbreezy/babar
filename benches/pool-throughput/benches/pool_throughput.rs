use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

use babar::codec::int4;
use babar::query::Query;
use babar::{Config, HealthCheck, Pool, PoolConfig, Session};
use bb8::Pool as Bb8Pool;
use bb8_postgres::PostgresConnectionManager;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use tokio::runtime::Builder;
use tokio_postgres::NoTls;

const DEFAULT_IMAGE: &str = "postgres:17-alpine";
const SQL: &str = "SELECT $1::int4 + 1";

#[derive(Debug)]
struct PgContainer {
    name: String,
    port: u16,
    user: String,
    password: String,
    db: String,
}

impl PgContainer {
    async fn start() -> Self {
        let suffix = format!(
            "{:x}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock before unix epoch")
                .as_nanos()
        );
        let name = format!("babar-pool-bench-{suffix}");
        let user = "babar".to_string();
        let password = "secret".to_string();
        let db = "babar".to_string();
        let image = std::env::var("BABAR_PG_IMAGE").unwrap_or_else(|_| DEFAULT_IMAGE.to_string());

        let status = Command::new("docker")
            .arg("info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        assert!(
            status.is_ok_and(|exit| exit.success()),
            "docker is required"
        );

        let out = Command::new("docker")
            .args([
                "run",
                "-d",
                "--rm",
                "--name",
                &name,
                "-p",
                "127.0.0.1::5432",
                "-e",
                &format!("POSTGRES_USER={user}"),
                "-e",
                &format!("POSTGRES_PASSWORD={password}"),
                "-e",
                &format!("POSTGRES_DB={db}"),
                "-e",
                "POSTGRES_HOST_AUTH_METHOD=scram-sha-256",
                &image,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .expect("docker run");
        assert!(
            out.status.success(),
            "docker run failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let port_out = Command::new("docker")
            .args(["port", &name, "5432/tcp"])
            .output()
            .expect("docker port");
        assert!(
            port_out.status.success(),
            "docker port failed: {}",
            String::from_utf8_lossy(&port_out.stderr)
        );
        let stdout = String::from_utf8_lossy(&port_out.stdout);
        let port = stdout
            .lines()
            .find_map(|line| {
                line.split(':')
                    .next_back()
                    .and_then(|segment| segment.trim().parse::<u16>().ok())
            })
            .unwrap_or_else(|| panic!("could not parse docker port output: {stdout:?}"));

        let pg = Self {
            name,
            port,
            user,
            password,
            db,
        };
        pg.wait_ready().await;
        pg
    }

    fn babar_config(&self, application_name: &str) -> Config {
        Config::new("127.0.0.1", self.port, &self.user, &self.db)
            .password(&self.password)
            .application_name(application_name)
    }

    fn tokio_postgres_config(&self, application_name: &str) -> tokio_postgres::Config {
        let mut cfg = tokio_postgres::Config::new();
        cfg.host("127.0.0.1");
        cfg.port(self.port);
        cfg.user(&self.user);
        cfg.password(&self.password);
        cfg.dbname(&self.db);
        cfg.application_name(application_name);
        cfg
    }

    fn sqlx_connect_options(&self, application_name: &str) -> PgConnectOptions {
        PgConnectOptions::new()
            .host("127.0.0.1")
            .port(self.port)
            .username(&self.user)
            .password(&self.password)
            .database(&self.db)
            .application_name(application_name)
    }

    async fn wait_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut last_err = String::new();
        loop {
            if Instant::now() >= deadline {
                panic!("container {} did not become ready: {last_err}", self.name);
            }
            match Session::connect(self.babar_config("pool-throughput-ready")).await {
                Ok(session) => {
                    session.close().await.expect("close readiness session");
                    return;
                }
                Err(err) => {
                    last_err = err.to_string();
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    }
}

impl Drop for PgContainer {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

struct BenchHarness {
    _pg: PgContainer,
    babar: Pool,
    bb8: Bb8Pool<PostgresConnectionManager<NoTls>>,
    sqlx: PgPool,
}

impl BenchHarness {
    async fn start() -> Self {
        let pg = PgContainer::start().await;
        let babar = Pool::new(
            pg.babar_config("pool-throughput-babar"),
            PoolConfig::new()
                .min_idle(10)
                .max_size(10)
                .acquire_timeout(Duration::from_secs(5))
                .health_check(HealthCheck::Ping),
        )
        .await
        .expect("build babar pool");
        let bb8 = Bb8Pool::builder()
            .max_size(10)
            .build(PostgresConnectionManager::new(
                pg.tokio_postgres_config("pool-throughput-bb8"),
                NoTls,
            ))
            .await
            .expect("build bb8 pool");
        let sqlx = PgPoolOptions::new()
            .min_connections(10)
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(pg.sqlx_connect_options("pool-throughput-sqlx"))
            .await
            .expect("build sqlx pool");
        Self {
            _pg: pg,
            babar,
            bb8,
            sqlx,
        }
    }

    async fn close(self) {
        self.babar.close().await;
        self.sqlx.close().await;
    }
}

fn pool_throughput(c: &mut Criterion) {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build runtime");
    let harness = runtime.block_on(BenchHarness::start());
    let babar_counter = AtomicI32::new(0);
    let bb8_counter = AtomicI32::new(0);
    let sqlx_counter = AtomicI32::new(0);
    let babar_query: Query<(i32,), (i32,)> = Query::raw(SQL, (int4,), (int4,));

    let mut group = c.benchmark_group("pool_throughput");
    group.sample_size(10);

    group.bench_function("babar/acquire_and_query", |b| {
        let pool = &harness.babar;
        let query = &babar_query;
        b.to_async(&runtime).iter(|| async {
            let value = babar_counter.fetch_add(1, Ordering::Relaxed);
            let conn = pool.acquire().await.expect("acquire babar connection");
            let rows = conn
                .query(query, (black_box(value),))
                .await
                .expect("run babar query");
            black_box(rows[0].0)
        });
    });

    group.bench_function("bb8_tokio_postgres/acquire_and_query", |b| {
        let pool = &harness.bb8;
        b.to_async(&runtime).iter(|| async {
            let value = bb8_counter.fetch_add(1, Ordering::Relaxed);
            let conn = pool.get().await.expect("acquire bb8 connection");
            let row = conn
                .query_one(SQL, &[&black_box(value)])
                .await
                .expect("run bb8 query");
            black_box(row.get::<_, i32>(0))
        });
    });

    group.bench_function("sqlx_pgpool/acquire_and_query", |b| {
        let pool = &harness.sqlx;
        b.to_async(&runtime).iter(|| async {
            let value = sqlx_counter.fetch_add(1, Ordering::Relaxed);
            let row: i32 = sqlx::query_scalar(SQL)
                .bind(black_box(value))
                .fetch_one(pool)
                .await
                .expect("run sqlx query");
            black_box(row)
        });
    });

    group.finish();
    runtime.block_on(harness.close());
}

criterion_group!(benches, pool_throughput);
criterion_main!(benches);
