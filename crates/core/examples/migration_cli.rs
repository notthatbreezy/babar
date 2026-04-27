//! Thin migration CLI over babar's shared migration engine.
//!
//! ```text
//! cargo run -p babar --example migration_cli -- --help
//! cargo run -p babar --example migration_cli -- status
//! cargo run -p babar --example migration_cli -- plan
//! cargo run -p babar --example migration_cli -- up
//! cargo run -p babar --example migration_cli -- down --steps 1
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use babar::migration::{
    FileSystemMigrationSource, MigrationPlan, MigrationPlanStep, MigrationStatus,
    MigrationStatusState, MigrationTable, MigrationTransactionMode,
    DEFAULT_MIGRATION_ADVISORY_LOCK_ID, DEFAULT_MIGRATION_SCHEMA, DEFAULT_MIGRATION_TABLE,
};
use babar::{Config, Migrator, MigratorOptions, Session};
use clap::{Args, Parser, Subcommand};

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Debug, Parser, Clone)]
#[command(name = "babar-migrate")]
#[command(about = "Apply and inspect babar SQL migrations")]
struct Cli {
    #[arg(long, env = "PGHOST", default_value = "127.0.0.1")]
    host: String,
    #[arg(long, env = "PGPORT", default_value_t = 5432)]
    port: u16,
    #[arg(long, env = "PGUSER", default_value = "postgres")]
    user: String,
    #[arg(long, env = "PGDATABASE", default_value = "postgres")]
    database: String,
    #[arg(long, env = "PGPASSWORD", default_value = "postgres")]
    password: String,
    #[arg(
        long,
        env = "BABAR_MIGRATIONS_DIR",
        default_value = "migrations",
        help = "Directory containing paired <version>__<name>.up.sql / .down.sql files"
    )]
    migrations_dir: PathBuf,
    #[arg(
        long,
        env = "BABAR_MIGRATION_SCHEMA",
        default_value = DEFAULT_MIGRATION_SCHEMA
    )]
    migration_schema: String,
    #[arg(
        long,
        env = "BABAR_MIGRATION_TABLE",
        default_value = DEFAULT_MIGRATION_TABLE
    )]
    migration_table: String,
    #[arg(
        long,
        env = "BABAR_MIGRATION_LOCK_ID",
        default_value_t = DEFAULT_MIGRATION_ADVISORY_LOCK_ID
    )]
    migration_lock_id: i64,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand, Clone)]
enum Commands {
    /// Show applied and pending migrations.
    Status,
    /// Print the dry-run migration plan. Defaults to planning `up`.
    Plan {
        #[command(subcommand)]
        direction: Option<PlanCommand>,
    },
    /// Apply every pending migration.
    Up,
    /// Roll back applied migrations in reverse order.
    Down(DownArgs),
}

#[derive(Debug, Subcommand, Clone)]
enum PlanCommand {
    /// Plan pending `up` migrations.
    Up,
    /// Plan a rollback before executing `down`.
    Down(DownArgs),
}

#[derive(Debug, Args, Clone)]
struct DownArgs {
    #[arg(
        long,
        default_value_t = 1,
        value_parser = parse_steps,
        help = "Number of applied migrations to roll back"
    )]
    steps: usize,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match execute(cli).await {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

async fn execute(cli: Cli) -> babar::Result<String> {
    let config = config_from_cli(&cli);
    let migrator = migrator_from_cli(&cli)?;
    let session = Session::connect(config).await?;
    let output = execute_command(&session, &migrator, &cli.command).await;
    let close = session.close().await;

    match (output, close) {
        (Ok(output), Ok(())) => Ok(output),
        (Err(err), Ok(()) | Err(_)) | (Ok(_), Err(err)) => Err(err),
    }
}

fn config_from_cli(cli: &Cli) -> Config {
    Config::new(&cli.host, cli.port, &cli.user, &cli.database)
        .password(cli.password.clone())
        .application_name("babar-migration-cli")
}

fn migrator_from_cli(cli: &Cli) -> babar::Result<Migrator<FileSystemMigrationSource>> {
    let table = MigrationTable::new(cli.migration_schema.clone(), cli.migration_table.clone())?;
    let options = MigratorOptions::new()
        .table(table)
        .advisory_lock_id(cli.migration_lock_id);
    Ok(Migrator::with_options(
        FileSystemMigrationSource::new(cli.migrations_dir.clone()),
        options,
    ))
}

async fn execute_command(
    session: &Session,
    migrator: &Migrator<FileSystemMigrationSource>,
    command: &Commands,
) -> babar::Result<String> {
    match command {
        Commands::Status => {
            let applied = migrator.applied_migrations(session).await?;
            let status = migrator.status(&applied)?;
            Ok(render_status(&status))
        }
        Commands::Plan { direction } => {
            let applied = migrator.applied_migrations(session).await?;
            let output = match direction {
                None | Some(PlanCommand::Up) => {
                    render_plan("apply plan", &migrator.plan_apply(&applied)?)
                }
                Some(PlanCommand::Down(args)) => render_plan(
                    "rollback plan",
                    &migrator.plan_rollback(&applied, args.steps)?,
                ),
            };
            Ok(output)
        }
        Commands::Up => {
            let plan = migrator.apply(session).await?;
            Ok(render_execution(
                "applied",
                &plan,
                "database is already at the latest migration",
            ))
        }
        Commands::Down(args) => {
            let plan = migrator.rollback(session, args.steps).await?;
            Ok(render_execution(
                "rolled back",
                &plan,
                "no applied migrations to roll back",
            ))
        }
    }
}

fn render_status(status: &MigrationStatus) -> String {
    let current = status
        .entries()
        .iter()
        .rev()
        .find_map(|entry| match entry.state() {
            MigrationStatusState::Applied { .. } => Some(entry.pair().id().to_string()),
            MigrationStatusState::Pending => None,
        });
    let next = status
        .entries()
        .iter()
        .find_map(|entry| match entry.state() {
            MigrationStatusState::Pending => Some(entry.pair().id().to_string()),
            MigrationStatusState::Applied { .. } => None,
        });

    let mut lines = vec![format!(
        "status: {} applied, {} pending",
        status.applied_count(),
        status.pending_count()
    )];

    lines.push(format!(
        "current: {}",
        current.unwrap_or_else(|| "<none>".to_string())
    ));
    lines.push(format!(
        "next: {}",
        next.unwrap_or_else(|| "<none>".to_string())
    ));

    for entry in status.entries() {
        let mode = format_script_modes(entry.pair());
        match entry.state() {
            MigrationStatusState::Applied { .. } => {
                lines.push(format!("APPLIED {id} ({mode})", id = entry.pair().id()));
            }
            MigrationStatusState::Pending => {
                lines.push(format!("PENDING {id} ({mode})", id = entry.pair().id()));
            }
        }
    }

    lines.join("\n")
}

fn render_plan(title: &str, plan: &MigrationPlan) -> String {
    if plan.is_empty() {
        return format!("{title}: no changes");
    }

    let mut lines = vec![format!("{title}: {} step(s)", plan.steps().len())];
    lines.extend(plan.steps().iter().map(render_plan_step));
    lines.join("\n")
}

fn render_execution(verb: &str, plan: &MigrationPlan, empty_message: &str) -> String {
    if plan.is_empty() {
        return empty_message.to_string();
    }

    let mut lines = vec![format!("{verb} {} migration(s)", plan.steps().len())];
    lines.extend(plan.steps().iter().map(render_plan_step));
    lines.join("\n")
}

fn render_plan_step(step: &MigrationPlanStep) -> String {
    let pair = step.pair();
    let mode = match step.kind() {
        babar::migration::MigrationKind::Up => pair.up().metadata().transaction_mode(),
        babar::migration::MigrationKind::Down => pair.down().metadata().transaction_mode(),
    };
    let direction = match step {
        MigrationPlanStep::Apply { .. } => "UP  ",
        MigrationPlanStep::Rollback { .. } => "DOWN",
    };
    format!(
        "{direction} {} ({})",
        pair.id(),
        format_transaction_mode(mode)
    )
}

fn format_script_modes(pair: &babar::migration::MigrationPair) -> String {
    format!(
        "up={}, down={}",
        format_transaction_mode(pair.up().metadata().transaction_mode()),
        format_transaction_mode(pair.down().metadata().transaction_mode())
    )
}

fn format_transaction_mode(mode: MigrationTransactionMode) -> &'static str {
    match mode {
        MigrationTransactionMode::Transactional => "transactional",
        MigrationTransactionMode::NonTransactional => "non-transactional",
    }
}

fn parse_steps(value: &str) -> Result<usize, String> {
    let steps = value
        .parse::<usize>()
        .map_err(|err| format!("invalid rollback step count: {err}"))?;
    if steps == 0 {
        Err("rollback steps must be at least 1".to_string())
    } else {
        Ok(steps)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::common::{AuthMode, PgContainer};
    use babar::codec::int8;
    use babar::migration::{MemoryMigrationSource, MigrationAsset};
    use babar::query::Query;
    use babar::Migrator;
    use clap::Parser;

    use super::{Cli, Commands, DownArgs, PlanCommand};

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

    fn unique_lock_id() -> i64 {
        0x0062_6162_0000_0000_i64 | i64::from(rand::random::<u32>())
    }

    #[test]
    fn plan_defaults_to_up() {
        let cli = Cli::try_parse_from(["migration_cli", "plan"]).expect("parse cli");
        assert!(matches!(cli.command, Commands::Plan { direction: None }));
    }

    #[test]
    fn down_rejects_zero_steps() {
        let err = Cli::try_parse_from(["migration_cli", "down", "--steps", "0"])
            .expect_err("zero rollback steps should fail");
        assert!(err.to_string().contains("at least 1"));
    }

    #[test]
    fn status_output_marks_applied_and_pending_migrations() {
        let migrator = Migrator::new(MemoryMigrationSource::new(vec![
            MigrationAsset::new("1__create_widgets.up.sql", "SELECT 1;"),
            MigrationAsset::new("1__create_widgets.down.sql", "SELECT 1;"),
            MigrationAsset::new("2__seed_widgets.up.sql", "SELECT 2;"),
            MigrationAsset::new("2__seed_widgets.down.sql", "SELECT 2;"),
        ]));
        let applied = vec![babar::migration::AppliedMigration::new(
            babar::migration::MigrationId::new(1, "create_widgets").unwrap(),
            babar::migration::MigrationChecksum::of_contents("SELECT 1;"),
            babar::migration::MigrationChecksum::of_contents("SELECT 1;"),
            babar::migration::MigrationTransactionMode::Transactional,
            babar::migration::MigrationTransactionMode::Transactional,
            SystemTime::UNIX_EPOCH,
        )];
        let status = migrator.status(&applied).expect("build status");
        let output = super::render_status(&status);

        assert!(output.contains("status: 1 applied, 1 pending"));
        assert!(output.contains("current: 1__create_widgets"));
        assert!(output.contains("next: 2__seed_widgets"));
        assert!(output.contains("APPLIED 1__create_widgets"));
        assert!(output.contains("PENDING 2__seed_widgets"));
    }

    #[tokio::test]
    async fn execute_runs_status_plan_up_and_down() {
        if !require_docker() {
            return;
        }

        let pg = PgContainer::start(AuthMode::Scram).await;
        let migrations = TestDir::new("migration-cli");
        migrations.write(
            "1__create_widgets.up.sql",
            "CREATE TABLE widgets (id int8 PRIMARY KEY, note text NOT NULL);",
        );
        migrations.write("1__create_widgets.down.sql", "DROP TABLE widgets;");
        migrations.write(
            "2__seed_widgets.up.sql",
            "INSERT INTO widgets (id, note) VALUES (1, 'seeded');",
        );
        migrations.write(
            "2__seed_widgets.down.sql",
            "DELETE FROM widgets WHERE id = 1;",
        );

        let table_name = format!("babar_schema_migrations_cli_{}", rand::random::<u32>());
        let base_cli = Cli {
            host: "127.0.0.1".to_string(),
            port: pg.port(),
            user: pg.user().to_string(),
            database: "babar".to_string(),
            password: pg.password().to_string(),
            migrations_dir: migrations.path().to_path_buf(),
            migration_schema: "public".to_string(),
            migration_table: table_name.clone(),
            migration_lock_id: unique_lock_id(),
            command: Commands::Status,
        };

        let status_before = super::execute(base_cli.clone())
            .await
            .expect("status before");
        assert!(status_before.contains("status: 0 applied, 2 pending"));

        let apply_plan = super::execute(Cli {
            command: Commands::Plan { direction: None },
            ..base_cli.clone()
        })
        .await
        .expect("plan apply");
        assert!(apply_plan.contains("apply plan: 2 step(s)"));
        assert!(apply_plan.contains("UP   1__create_widgets"));
        assert!(apply_plan.contains("UP   2__seed_widgets"));

        let applied = super::execute(Cli {
            command: Commands::Up,
            ..base_cli.clone()
        })
        .await
        .expect("apply migrations");
        assert!(applied.contains("applied 2 migration(s)"));

        let session = babar::Session::connect(
            pg.config(pg.user(), pg.password())
                .application_name("babar-migration-cli-test"),
        )
        .await
        .expect("connect validation session");
        assert_eq!(
            count_rows(&session, "SELECT COUNT(*)::int8 FROM widgets").await,
            1
        );
        assert_eq!(
            count_rows(
                &session,
                &format!("SELECT COUNT(*)::int8 FROM public.\"{table_name}\""),
            )
            .await,
            2
        );
        session.close().await.expect("close validation session");

        let rollback_plan = super::execute(Cli {
            command: Commands::Plan {
                direction: Some(PlanCommand::Down(DownArgs { steps: 1 })),
            },
            ..base_cli.clone()
        })
        .await
        .expect("plan rollback");
        assert!(rollback_plan.contains("rollback plan: 1 step(s)"));
        assert!(rollback_plan.contains("DOWN 2__seed_widgets"));

        let rolled_back = super::execute(Cli {
            command: Commands::Down(DownArgs { steps: 1 }),
            ..base_cli
        })
        .await
        .expect("rollback migration");
        assert!(rolled_back.contains("rolled back 1 migration(s)"));

        let session = babar::Session::connect(
            pg.config(pg.user(), pg.password())
                .application_name("babar-migration-cli-test-post"),
        )
        .await
        .expect("connect validation session");
        assert_eq!(
            count_rows(
                &session,
                &format!("SELECT COUNT(*)::int8 FROM public.\"{table_name}\"")
            )
            .await,
            1
        );
        assert_eq!(
            count_rows(&session, "SELECT COUNT(*)::int8 FROM widgets").await,
            0
        );
        session.close().await.expect("close validation session");
    }

    async fn count_rows(session: &babar::Session, sql: &str) -> i64 {
        let query: Query<(), (i64,)> = Query::raw(sql, (), (int8,));
        session.query(&query, ()).await.expect("count rows")[0].0
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
}
