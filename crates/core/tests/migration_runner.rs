//! Integration tests for the shared migration execution engine.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use babar::codec::{int8, nullable, text};
use babar::migration::{
    FileSystemMigrationSource, MemoryMigrationSource, MigrationAsset, MigrationError,
};
use babar::query::Query;
use babar::{Error, Migrator, MigratorOptions, Session};
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

async fn fresh_session(application_name: &str) -> Option<(PgContainer, Session)> {
    if !require_docker() {
        return None;
    }

    let pg = PgContainer::start(AuthMode::Scram).await;
    let session = Session::connect(
        pg.config(pg.user(), pg.password())
            .application_name(application_name),
    )
    .await
    .expect("connect session");
    Some((pg, session))
}

fn unique_lock_id() -> i64 {
    0x0062_6162_0000_0000_i64 | i64::from(rand::random::<u32>())
}

fn migrator(assets: Vec<MigrationAsset>) -> Migrator<MemoryMigrationSource> {
    Migrator::with_options(
        MemoryMigrationSource::new(assets),
        MigratorOptions::new().advisory_lock_id(unique_lock_id()),
    )
}

async fn count_rows(session: &Session, sql: &str) -> i64 {
    let query: Query<(), (i64,)> = Query::raw(sql, (), (int8,));
    session.query(&query, ()).await.expect("count rows")[0].0
}

async fn optional_text(session: &Session, sql: &str) -> Option<String> {
    let query: Query<(), (Option<String>,)> = Query::raw(sql, (), (nullable(text),));
    session
        .query(&query, ())
        .await
        .expect("query optional text")[0]
        .0
        .clone()
}

#[tokio::test]
async fn apply_is_idempotent_and_records_history() {
    let Some((_pg, session)) = fresh_session("babar-migration-runner-apply").await else {
        return;
    };

    let migrator = migrator(vec![
        MigrationAsset::new(
            "1__create_widgets.up.sql",
            "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL);",
        ),
        MigrationAsset::new("1__create_widgets.down.sql", "DROP TABLE widgets;"),
        MigrationAsset::new(
            "2__seed_widgets.up.sql",
            "INSERT INTO widgets (id, note) VALUES (1, 'seeded');",
        ),
        MigrationAsset::new(
            "2__seed_widgets.down.sql",
            "DELETE FROM widgets WHERE id = 1;",
        ),
    ]);

    let plan = migrator.apply(&session).await.expect("apply migrations");
    assert_eq!(plan.steps().len(), 2);
    assert_eq!(
        migrator
            .applied_migrations(&session)
            .await
            .expect("load applied rows")
            .len(),
        2
    );
    assert_eq!(
        count_rows(&session, "SELECT COUNT(*)::int8 FROM widgets").await,
        1
    );
    assert_eq!(
        count_rows(
            &session,
            "SELECT COUNT(*)::int8 FROM public.babar_schema_migrations",
        )
        .await,
        2
    );

    let noop = migrator.apply(&session).await.expect("reapply migrations");
    assert!(noop.is_empty());
    assert_eq!(
        count_rows(&session, "SELECT COUNT(*)::int8 FROM widgets").await,
        1
    );

    session.close().await.expect("close session");
}

#[tokio::test]
async fn rollback_reverses_latest_migration_only() {
    let Some((_pg, session)) = fresh_session("babar-migration-runner-rollback").await else {
        return;
    };

    let migrator = migrator(vec![
        MigrationAsset::new(
            "1__create_widgets.up.sql",
            "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL);",
        ),
        MigrationAsset::new("1__create_widgets.down.sql", "DROP TABLE widgets;"),
        MigrationAsset::new(
            "2__seed_widgets.up.sql",
            "INSERT INTO widgets (id, note) VALUES (1, 'seeded');",
        ),
        MigrationAsset::new(
            "2__seed_widgets.down.sql",
            "DELETE FROM widgets WHERE id = 1;",
        ),
    ]);

    migrator.apply(&session).await.expect("apply migrations");
    let plan = migrator
        .rollback(&session, 1)
        .await
        .expect("rollback migration");
    assert_eq!(plan.steps().len(), 1);
    assert_eq!(plan.steps()[0].pair().id().version(), 2);
    assert_eq!(
        count_rows(&session, "SELECT COUNT(*)::int8 FROM widgets").await,
        0
    );
    assert_eq!(
        count_rows(
            &session,
            "SELECT COUNT(*)::int8 FROM public.babar_schema_migrations",
        )
        .await,
        1
    );

    session.close().await.expect("close session");
}

#[tokio::test]
async fn non_transactional_migrations_run_outside_explicit_transactions() {
    let Some((_pg, session)) = fresh_session("babar-migration-runner-non-tx").await else {
        return;
    };

    let migrator = migrator(vec![
        MigrationAsset::new(
            "1__create_widgets.up.sql",
            "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL);",
        ),
        MigrationAsset::new("1__create_widgets.down.sql", "DROP TABLE widgets;"),
        MigrationAsset::new(
            "2__widgets_idx.up.sql",
            "--! babar:transaction = none\nCREATE INDEX CONCURRENTLY widgets_note_idx ON widgets (note);",
        ),
        MigrationAsset::new(
            "2__widgets_idx.down.sql",
            "--! babar:transaction = none\nDROP INDEX CONCURRENTLY widgets_note_idx;",
        ),
    ]);

    migrator.apply(&session).await.expect("apply migrations");
    assert_eq!(
        optional_text(
            &session,
            "SELECT to_regclass('public.widgets_note_idx')::text",
        )
        .await,
        Some("widgets_note_idx".to_string())
    );

    migrator
        .rollback(&session, 1)
        .await
        .expect("rollback non-transactional migration");
    assert_eq!(
        optional_text(
            &session,
            "SELECT to_regclass('public.widgets_note_idx')::text",
        )
        .await,
        None
    );
    assert_eq!(
        optional_text(&session, "SELECT to_regclass('public.widgets')::text").await,
        Some("widgets".to_string())
    );

    session.close().await.expect("close session");
}

#[tokio::test]
async fn transactional_failures_roll_back_script_and_history_row() {
    let Some((_pg, session)) = fresh_session("babar-migration-runner-failure").await else {
        return;
    };

    let migrator = migrator(vec![
        MigrationAsset::new(
            "1__create_widgets.up.sql",
            "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL);",
        ),
        MigrationAsset::new("1__create_widgets.down.sql", "DROP TABLE widgets;"),
        MigrationAsset::new(
            "2__broken_seed.up.sql",
            "INSERT INTO widgets (id, note) VALUES (1, 'seeded'); SELECT does_not_exist FROM widgets;",
        ),
        MigrationAsset::new(
            "2__broken_seed.down.sql",
            "DELETE FROM widgets WHERE id = 1;",
        ),
    ]);

    let err = migrator
        .apply(&session)
        .await
        .expect_err("migration should fail");
    assert!(matches!(err, Error::Server { .. }));
    assert_eq!(
        count_rows(&session, "SELECT COUNT(*)::int8 FROM widgets").await,
        0
    );
    assert_eq!(
        count_rows(
            &session,
            "SELECT COUNT(*)::int8 FROM public.babar_schema_migrations",
        )
        .await,
        1
    );

    session.close().await.expect("close session");
}

#[tokio::test]
async fn drift_is_checked_before_pending_execution() {
    let Some((_pg, session)) = fresh_session("babar-migration-runner-drift").await else {
        return;
    };

    let original = migrator(vec![
        MigrationAsset::new(
            "1__create_widgets.up.sql",
            "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL);",
        ),
        MigrationAsset::new("1__create_widgets.down.sql", "DROP TABLE widgets;"),
    ]);
    original
        .apply(&session)
        .await
        .expect("apply baseline migration");

    let drifted = migrator(vec![
        MigrationAsset::new(
            "1__create_widgets.up.sql",
            "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL, extra text);",
        ),
        MigrationAsset::new("1__create_widgets.down.sql", "DROP TABLE widgets;"),
        MigrationAsset::new(
            "2__seed_widgets.up.sql",
            "INSERT INTO widgets (id, note) VALUES (1, 'seeded');",
        ),
        MigrationAsset::new(
            "2__seed_widgets.down.sql",
            "DELETE FROM widgets WHERE id = 1;",
        ),
    ]);

    let err = drifted
        .apply(&session)
        .await
        .expect_err("drift should fail before execution");
    assert!(matches!(
        err,
        Error::Migration(MigrationError::DriftDetected { .. })
    ));
    assert_eq!(
        count_rows(&session, "SELECT COUNT(*)::int8 FROM widgets").await,
        0
    );
    assert_eq!(
        count_rows(
            &session,
            "SELECT COUNT(*)::int8 FROM public.babar_schema_migrations",
        )
        .await,
        1
    );

    session.close().await.expect("close session");
}

#[tokio::test]
async fn advisory_lock_serializes_concurrent_apply_calls() {
    if !require_docker() {
        return;
    }

    let pg = PgContainer::start(AuthMode::Scram).await;
    let session1 = Session::connect(
        pg.config(pg.user(), pg.password())
            .application_name("babar-migration-runner-lock-1"),
    )
    .await
    .expect("connect session1");
    let session2 = Session::connect(
        pg.config(pg.user(), pg.password())
            .application_name("babar-migration-runner-lock-2"),
    )
    .await
    .expect("connect session2");

    let lock_id = unique_lock_id();
    let migrator = Migrator::with_options(
        MemoryMigrationSource::new(vec![
            MigrationAsset::new(
                "1__create_widgets.up.sql",
                "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL);",
            ),
            MigrationAsset::new("1__create_widgets.down.sql", "DROP TABLE widgets;"),
        ]),
        MigratorOptions::new().advisory_lock_id(lock_id),
    );

    session1
        .simple_query_raw(&format!("SELECT pg_advisory_lock({lock_id})"))
        .await
        .expect("acquire manual advisory lock");

    let task = tokio::spawn({
        let migrator = migrator.clone();
        async move { migrator.apply(&session2).await }
    });

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !task.is_finished(),
        "apply should wait for the advisory lock"
    );

    session1
        .simple_query_raw(&format!("SELECT pg_advisory_unlock({lock_id})"))
        .await
        .expect("release manual advisory lock");

    let plan = task.await.expect("join task").expect("apply after unlock");
    assert_eq!(plan.steps().len(), 1);
    assert_eq!(
        optional_text(&session1, "SELECT to_regclass('public.widgets')::text").await,
        Some("widgets".to_string())
    );

    session1.close().await.expect("close session1");
}

#[tokio::test]
async fn filesystem_source_supports_startup_apply_and_drift_detection() {
    let Some((_pg, session)) = fresh_session("babar-migration-runner-filesystem").await else {
        return;
    };

    let dir = TestDir::new("migration-runner-filesystem");
    dir.write(
        "1__create_widgets.up.sql",
        "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL);",
    );
    dir.write("1__create_widgets.down.sql", "DROP TABLE widgets;");

    let migrator = Migrator::with_options(
        FileSystemMigrationSource::new(dir.path()),
        MigratorOptions::new().advisory_lock_id(unique_lock_id()),
    );

    let plan = migrator
        .apply(&session)
        .await
        .expect("apply filesystem migrations");
    assert_eq!(plan.steps().len(), 1);
    assert_eq!(
        count_rows(
            &session,
            "SELECT COUNT(*)::int8 FROM public.babar_schema_migrations",
        )
        .await,
        1
    );

    dir.write(
        "1__create_widgets.up.sql",
        "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL, extra text);",
    );
    dir.write(
        "2__seed_widgets.up.sql",
        "INSERT INTO widgets (id, note) VALUES (1, 'seeded');",
    );
    dir.write(
        "2__seed_widgets.down.sql",
        "DELETE FROM widgets WHERE id = 1;",
    );

    let err = migrator
        .apply(&session)
        .await
        .expect_err("drifted filesystem migrations should fail");
    assert!(matches!(
        err,
        Error::Migration(MigrationError::DriftDetected { .. })
    ));
    assert_eq!(
        count_rows(
            &session,
            "SELECT COUNT(*)::int8 FROM public.babar_schema_migrations",
        )
        .await,
        1
    );

    session.close().await.expect("close session");
}

#[tokio::test]
async fn rollback_caps_requested_steps_to_applied_prefix() {
    let Some((_pg, session)) = fresh_session("babar-migration-runner-rollback-cap").await else {
        return;
    };

    let migrator = migrator(vec![
        MigrationAsset::new(
            "1__create_widgets.up.sql",
            "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL);",
        ),
        MigrationAsset::new("1__create_widgets.down.sql", "DROP TABLE widgets;"),
        MigrationAsset::new(
            "2__seed_widgets.up.sql",
            "INSERT INTO widgets (id, note) VALUES (1, 'seeded');",
        ),
        MigrationAsset::new(
            "2__seed_widgets.down.sql",
            "DELETE FROM widgets WHERE id = 1;",
        ),
    ]);

    migrator.apply(&session).await.expect("apply migrations");
    let plan = migrator
        .rollback(&session, 10)
        .await
        .expect("rollback applied prefix");
    assert_eq!(plan.steps().len(), 2);
    assert_eq!(plan.steps()[0].pair().id().version(), 2);
    assert_eq!(plan.steps()[1].pair().id().version(), 1);
    assert_eq!(
        optional_text(&session, "SELECT to_regclass('public.widgets')::text").await,
        None
    );
    assert_eq!(
        count_rows(
            &session,
            "SELECT COUNT(*)::int8 FROM public.babar_schema_migrations",
        )
        .await,
        0
    );

    session.close().await.expect("close session");
}

#[derive(Debug)]
struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("monotonic time")
            .as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-artifacts")
            .join(format!("{label}-{unique}"));
        std::fs::create_dir_all(&path).expect("create test dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.path.join(name), contents).expect("write migration file");
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
