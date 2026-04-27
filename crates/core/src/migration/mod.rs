//! Shared architecture for babar's PostgreSQL schema-migration subsystem.
//!
//! The migration engine is intentionally library-first: the future CLI will be
//! a thin wrapper over the same [`Migrator`] and [`MigrationSource`] types.
//!
//! ## File grammar
//!
//! babar uses paired SQL files with the following names:
//!
//! ```text
//! <version>__<name>.up.sql
//! <version>__<name>.down.sql
//! ```
//!
//! - `version` is an unsigned integer used for ordering
//! - `name` is lowercase `snake_case`
//! - every migration must provide both an `up` and a `down` file
//!
//! ## Startup-safe library API
//!
//! Use [`Migrator::apply`] during process startup before serving traffic:
//!
//! ```rust,no_run
//! use babar::migration::FileSystemMigrationSource;
//! use babar::{Config, Migrator, Session};
//!
//! # async fn demo() -> babar::Result<()> {
//! let session = Session::connect(
//!     Config::new("localhost", 5432, "postgres", "app").password("secret"),
//! )
//! .await?;
//! let migrator = Migrator::new(FileSystemMigrationSource::new("migrations"));
//! migrator.apply(&session).await?;
//! session.close().await?;
//! # Ok(())
//! # }
//! ```
//!
//! `apply` is idempotent, creates the migration state table if needed, and
//! serializes concurrent runners with PostgreSQL advisory locks so multiple app
//! instances can safely race through the same startup path.
//!
//! ## Non-transactional migrations
//!
//! Scripts run transactionally by default. A file opts out explicitly with a
//! top-of-file pragma:
//!
//! ```sql
//! --! babar:transaction = none
//! ```
//!
//! The pragma is per-file, so `up` and `down` may declare different execution
//! modes when PostgreSQL requires it (for example, `CREATE INDEX CONCURRENTLY`).
//! If a non-transactional script fails, PostgreSQL may keep the partial effects
//! of statements that already committed, but babar still leaves the migration
//! history row unapplied so the drift is visible and recovery stays explicit.
//!
//! ## Migration state table
//!
//! [`MigrationTable::create_if_missing_sql`] defines the canonical schema for
//! babar's state table. The default table is `public.babar_schema_migrations`
//! and stores the applied migration id, both file checksums, both transaction
//! modes, and the `applied_at` timestamp needed for status and rollback flows.
//!
//! ## Drift detection and rollback limits
//!
//! babar stores checksums for both the `up` and `down` files plus both recorded
//! transaction modes. Status, planning, apply, and rollback all reconcile the
//! applied prefix against the current source catalog and fail with
//! [`MigrationError::DriftDetected`] if any applied migration changed on disk.
//!
//! Rollbacks only operate on the currently applied prefix in reverse version
//! order. Asking to roll back more steps than are applied simply rolls back the
//! whole applied prefix; babar does not attempt arbitrary point-in-time schema
//! reconstruction beyond the checked-in `down` scripts.

mod checksum;
mod error;
mod filename;
mod model;
mod plan;
#[cfg(not(loom))]
mod runner;
mod source;
mod table;

pub use checksum::MigrationChecksum;
pub use error::MigrationError;
pub use filename::{MigrationFilename, MigrationId, MigrationKind};
pub use model::{
    AppliedMigration, MigrationPair, MigrationScript, MigrationScriptMetadata,
    MigrationTransactionMode,
};
pub use plan::{
    MigrationCatalog, MigrationPlan, MigrationPlanDirection, MigrationPlanStep, MigrationStatus,
    MigrationStatusEntry, MigrationStatusState,
};
#[cfg(not(loom))]
pub use runner::MigrationExecutor;
pub use source::{
    FileSystemMigrationSource, MemoryMigrationSource, MigrationAsset, MigrationSource,
};
pub use table::{
    MigrationTable, DEFAULT_MIGRATION_ADVISORY_LOCK_ID, DEFAULT_MIGRATION_SCHEMA,
    DEFAULT_MIGRATION_TABLE,
};

/// Convenience alias for migration-parsing and configuration operations.
pub type Result<T> = std::result::Result<T, MigrationError>;

/// Shared configuration for the migration engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratorOptions {
    table: MigrationTable,
    advisory_lock_id: i64,
}

impl Default for MigratorOptions {
    fn default() -> Self {
        Self {
            table: MigrationTable::default(),
            advisory_lock_id: DEFAULT_MIGRATION_ADVISORY_LOCK_ID,
        }
    }
}

impl MigratorOptions {
    /// Create migration options with babar's defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the migration state table.
    #[must_use]
    pub fn table(mut self, table: MigrationTable) -> Self {
        self.table = table;
        self
    }

    /// Override the advisory lock id used by the runner.
    #[must_use]
    pub fn advisory_lock_id(mut self, advisory_lock_id: i64) -> Self {
        self.advisory_lock_id = advisory_lock_id;
        self
    }

    /// Borrow the configured migration state table.
    #[must_use]
    pub fn migration_table(&self) -> &MigrationTable {
        &self.table
    }

    /// Advisory lock id reserved for migration coordination.
    #[must_use]
    pub const fn advisory_lock_id_value(&self) -> i64 {
        self.advisory_lock_id
    }
}

/// Library-first migration engine wrapper.
///
/// Later migration planning and execution work will layer on top of this type
/// instead of building a second, CLI-specific implementation.
#[derive(Debug, Clone)]
pub struct Migrator<S> {
    source: S,
    options: MigratorOptions,
}

impl<S> Migrator<S> {
    /// Build a migrator with default options.
    #[must_use]
    pub fn new(source: S) -> Self {
        Self {
            source,
            options: MigratorOptions::default(),
        }
    }

    /// Build a migrator with explicit options.
    #[must_use]
    pub fn with_options(source: S, options: MigratorOptions) -> Self {
        Self { source, options }
    }

    /// Borrow the configured migration source.
    #[must_use]
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Borrow the configured migration options.
    #[must_use]
    pub fn options(&self) -> &MigratorOptions {
        &self.options
    }

    /// Consume the migrator and return the underlying source.
    #[must_use]
    pub fn into_source(self) -> S {
        self.source
    }
}

impl<S> Migrator<S>
where
    S: MigrationSource,
{
    /// Load, validate, and pair source migrations into a deterministic catalog.
    pub fn catalog(&self) -> Result<MigrationCatalog> {
        MigrationCatalog::from_source(&self.source)
    }

    /// Build migration status against the applied migration state table rows.
    pub fn status(&self, applied: &[AppliedMigration]) -> Result<MigrationStatus> {
        self.catalog()?.status(applied)
    }

    /// Build the dry-run plan required to reach the latest source migration.
    pub fn plan_apply(&self, applied: &[AppliedMigration]) -> Result<MigrationPlan> {
        self.catalog()?.plan_apply(applied)
    }

    /// Build the dry-run plan required to roll back the last `steps` migrations.
    pub fn plan_rollback(
        &self,
        applied: &[AppliedMigration],
        steps: usize,
    ) -> Result<MigrationPlan> {
        self.catalog()?.plan_rollback(applied, steps)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MemoryMigrationSource, MigrationAsset, MigrationChecksum, MigrationFilename, MigrationPair,
        MigrationPlanDirection, MigrationScript, MigrationScriptMetadata, MigrationSource,
        MigrationStatusState, MigrationTable, MigrationTransactionMode,
    };
    use std::time::SystemTime;

    #[test]
    fn parses_migration_filename() {
        let file = MigrationFilename::parse("20240623153000__create_users.up.sql").unwrap();
        assert_eq!(file.id().version(), 20_240_623_153_000);
        assert_eq!(file.id().name(), "create_users");
        assert_eq!(file.to_string(), "20240623153000__create_users.up.sql");
    }

    #[test]
    fn rejects_invalid_migration_filename() {
        let err = MigrationFilename::parse("create_users.sql").unwrap_err();
        assert!(err.to_string().contains("invalid migration file name"));
    }

    #[test]
    fn parses_non_transactional_directive() {
        let metadata =
            MigrationScriptMetadata::parse("--! babar:transaction = none\nSELECT 1;").unwrap();
        assert_eq!(
            metadata.transaction_mode(),
            MigrationTransactionMode::NonTransactional
        );
    }

    #[test]
    fn pairs_matching_up_and_down_scripts() {
        let up = MigrationScript::new(
            MigrationFilename::parse("1__bootstrap.up.sql").unwrap(),
            "SELECT 1;",
        )
        .unwrap();
        let down = MigrationScript::new(
            MigrationFilename::parse("1__bootstrap.down.sql").unwrap(),
            "SELECT 1;",
        )
        .unwrap();

        let pair = MigrationPair::new(up, down).unwrap();
        assert_eq!(pair.id().version(), 1);
        assert_eq!(
            pair.up().checksum(),
            MigrationChecksum::of_contents("SELECT 1;")
        );
    }

    #[test]
    fn memory_source_loads_assets() {
        let source = MemoryMigrationSource::new(vec![MigrationAsset::new(
            "1__bootstrap.up.sql",
            "SELECT 1;",
        )]);
        assert_eq!(source.load().unwrap().len(), 1);
    }

    #[test]
    fn renders_state_table_sql() {
        let sql = MigrationTable::default().create_if_missing_sql();
        assert!(sql.contains("\"public\".\"babar_schema_migrations\""));
        assert!(sql.contains("up_checksum text NOT NULL"));
        assert!(sql.contains("down_transaction_mode text NOT NULL"));
    }

    #[test]
    fn migrator_builds_status_and_plan() {
        let source = MemoryMigrationSource::new(vec![
            MigrationAsset::new("1__bootstrap.up.sql", "SELECT 1;"),
            MigrationAsset::new("1__bootstrap.down.sql", "SELECT 1;"),
            MigrationAsset::new("2__seed.up.sql", "SELECT 2;"),
            MigrationAsset::new("2__seed.down.sql", "SELECT 2;"),
        ]);
        let migrator = super::Migrator::new(source);
        let applied = vec![super::AppliedMigration::new(
            super::MigrationId::new(1, "bootstrap").unwrap(),
            MigrationChecksum::of_contents("SELECT 1;"),
            MigrationChecksum::of_contents("SELECT 1;"),
            MigrationTransactionMode::Transactional,
            MigrationTransactionMode::Transactional,
            SystemTime::UNIX_EPOCH,
        )];

        let status = migrator.status(&applied).unwrap();
        assert!(matches!(
            status.entries()[0].state(),
            MigrationStatusState::Applied { .. }
        ));
        assert!(matches!(
            status.entries()[1].state(),
            MigrationStatusState::Pending
        ));

        let plan = migrator.plan_apply(&applied).unwrap();
        assert_eq!(plan.direction(), MigrationPlanDirection::Apply);
        assert_eq!(plan.steps().len(), 1);
        assert_eq!(plan.steps()[0].pair().id().to_string(), "2__seed");
    }
}
