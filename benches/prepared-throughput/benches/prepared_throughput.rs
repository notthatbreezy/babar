use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

use babar::codec::int4;
use babar::query::Query;
use babar::{Config, PreparedQuery, Session};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::runtime::Builder;
use tokio::task::JoinHandle;
use tokio_postgres::{types::Type, Client, NoTls, Statement};

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
        let name = format!("babar-bench-{suffix}");
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
            "docker is required to run this benchmark"
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

    fn tokio_postgres_config(&self, application_name: &str) -> String {
        format!(
            "host=127.0.0.1 port={} user={} password={} dbname={} application_name={}",
            self.port, self.user, self.password, self.db, application_name
        )
    }

    async fn wait_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut last_err = String::new();
        loop {
            if Instant::now() >= deadline {
                panic!("container {} did not become ready: {last_err}", self.name);
            }
            match Session::connect(self.babar_config("prepared-throughput-ready")).await {
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

struct BabarState {
    session: Session,
    prepared: PreparedQuery<(i32,), (i32,)>,
}

impl BabarState {
    async fn connect(pg: &PgContainer) -> Self {
        let session = Session::connect(pg.babar_config("prepared-throughput-babar"))
            .await
            .expect("connect babar");
        let query: Query<(i32,), (i32,)> = Query::raw(SQL, (int4,), (int4,));
        let prepared = session.prepare_query(&query).await.expect("prepare babar");
        Self { session, prepared }
    }

    async fn close(self) {
        self.prepared.close().await.expect("close babar prepared");
        self.session.close().await.expect("close babar session");
    }
}

struct TokioPostgresState {
    client: Client,
    prepared: Statement,
    connection_task: JoinHandle<()>,
}

impl TokioPostgresState {
    async fn connect(pg: &PgContainer) -> Self {
        let (client, connection) = tokio_postgres::connect(
            &pg.tokio_postgres_config("prepared-throughput-tokio"),
            NoTls,
        )
        .await
        .expect("connect tokio-postgres");
        let connection_task = tokio::spawn(async move {
            if let Err(err) = connection.await {
                panic!("tokio-postgres connection failed: {err}");
            }
        });
        let prepared = client
            .prepare_typed(SQL, &[Type::INT4])
            .await
            .expect("prepare tokio-postgres");
        Self {
            client,
            prepared,
            connection_task,
        }
    }

    async fn close(self) {
        drop(self.client);
        self.connection_task.abort();
        let _ = self.connection_task.await;
    }
}

struct BenchHarness {
    _pg: PgContainer,
    babar: BabarState,
    tokio_postgres: TokioPostgresState,
}

impl BenchHarness {
    async fn start() -> Self {
        let pg = PgContainer::start().await;
        let babar = BabarState::connect(&pg).await;
        let tokio_postgres = TokioPostgresState::connect(&pg).await;
        Self {
            _pg: pg,
            babar,
            tokio_postgres,
        }
    }

    async fn close(self) {
        self.babar.close().await;
        self.tokio_postgres.close().await;
    }
}

fn prepared_statement_throughput(c: &mut Criterion) {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");
    let harness = runtime.block_on(BenchHarness::start());
    let babar_counter = AtomicI32::new(0);
    let tokio_counter = AtomicI32::new(0);

    let mut group = c.benchmark_group("prepared_statement_throughput");
    group.sample_size(10);

    group.bench_function("babar/select_int4_plus_one", |b| {
        let prepared = &harness.babar.prepared;
        b.to_async(&runtime).iter(|| async {
            let value = babar_counter.fetch_add(1, Ordering::Relaxed);
            let rows = prepared
                .query((black_box(value),))
                .await
                .expect("execute babar prepared statement");
            black_box(rows[0].0)
        });
    });

    group.bench_function("tokio_postgres/select_int4_plus_one", |b| {
        let client = &harness.tokio_postgres.client;
        let prepared = &harness.tokio_postgres.prepared;
        b.to_async(&runtime).iter(|| async {
            let value = tokio_counter.fetch_add(1, Ordering::Relaxed);
            let row = client
                .query_one(prepared, &[&black_box(value)])
                .await
                .expect("execute tokio-postgres prepared statement");
            black_box(row.get::<_, i32>(0))
        });
    });

    group.finish();
    runtime.block_on(harness.close());
}

criterion_group!(benches, prepared_statement_throughput);
criterion_main!(benches);
