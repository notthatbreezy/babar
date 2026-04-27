use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;

use super::{
    AppliedMigration, MigrationChecksum, MigrationError, MigrationId, MigrationPlan,
    MigrationPlanStep, MigrationSource, MigrationTable, MigrationTransactionMode, Migrator,
};
use crate::RawRows;

#[cfg(not(loom))]
use crate::{PoolConnection, Session};

mod private {
    #[cfg(not(loom))]
    pub trait Sealed {}

    #[cfg(not(loom))]
    impl Sealed for crate::Session {}

    #[cfg(not(loom))]
    impl Sealed for crate::PoolConnection {}
}

/// A connection-like value that the shared migration runner can execute against.
///
/// This trait is sealed to babar's built-in connection types so the shared
/// migration engine can support both [`crate::Session`] and
/// [`crate::PoolConnection`] without exposing duplicate APIs.
#[cfg(not(loom))]
#[allow(async_fn_in_trait)]
pub trait MigrationExecutor: private::Sealed {
    /// Run one SQL string with PostgreSQL's simple-query protocol.
    async fn simple_query_raw(&self, sql: &str) -> crate::Result<Vec<RawRows>>;

    /// Run multiple SQL strings inside one explicit transaction.
    async fn transactional_batch(&self, statements: &[String]) -> crate::Result<()>;
}

#[cfg(not(loom))]
impl MigrationExecutor for Session {
    async fn simple_query_raw(&self, sql: &str) -> crate::Result<Vec<RawRows>> {
        Session::simple_query_raw(self, sql).await
    }

    async fn transactional_batch(&self, statements: &[String]) -> crate::Result<()> {
        Session::simple_query_raw(self, "BEGIN").await?;
        for statement in statements {
            if let Err(err) = Session::simple_query_raw(self, statement).await {
                Session::simple_query_raw(self, "ROLLBACK")
                    .await
                    .map(|_| ())?;
                return Err(err);
            }
        }
        Session::simple_query_raw(self, "COMMIT").await.map(|_| ())
    }
}

#[cfg(not(loom))]
impl MigrationExecutor for PoolConnection {
    async fn simple_query_raw(&self, sql: &str) -> crate::Result<Vec<RawRows>> {
        PoolConnection::simple_query_raw(self, sql).await
    }

    async fn transactional_batch(&self, statements: &[String]) -> crate::Result<()> {
        PoolConnection::simple_query_raw(self, "BEGIN").await?;
        for statement in statements {
            if let Err(err) = PoolConnection::simple_query_raw(self, statement).await {
                PoolConnection::simple_query_raw(self, "ROLLBACK")
                    .await
                    .map(|_| ())?;
                return Err(err);
            }
        }
        PoolConnection::simple_query_raw(self, "COMMIT")
            .await
            .map(|_| ())
    }
}

#[cfg(not(loom))]
impl<S> Migrator<S>
where
    S: MigrationSource,
{
    /// Load applied migration rows from the configured state table.
    pub async fn applied_migrations<R>(&self, executor: &R) -> crate::Result<Vec<AppliedMigration>>
    where
        R: MigrationExecutor,
    {
        ensure_state_table(executor, self.options().migration_table()).await?;
        load_applied_migrations(executor, self.options().migration_table()).await
    }

    /// Apply every pending `up` migration in deterministic source order.
    pub async fn apply<R>(&self, executor: &R) -> crate::Result<MigrationPlan>
    where
        R: MigrationExecutor,
    {
        ensure_state_table(executor, self.options().migration_table()).await?;
        acquire_advisory_lock(executor, self.options().advisory_lock_id_value()).await?;

        let outcome = async {
            let applied =
                load_applied_migrations(executor, self.options().migration_table()).await?;
            let plan = self.plan_apply(&applied)?;
            execute_plan(executor, self.options().migration_table(), &plan).await?;
            Ok(plan)
        }
        .await;

        let unlock = release_advisory_lock(executor, self.options().advisory_lock_id_value()).await;

        finish_locked_operation(outcome, unlock)
    }

    /// Roll back the last `steps` applied migrations in reverse version order.
    pub async fn rollback<R>(&self, executor: &R, steps: usize) -> crate::Result<MigrationPlan>
    where
        R: MigrationExecutor,
    {
        ensure_state_table(executor, self.options().migration_table()).await?;
        acquire_advisory_lock(executor, self.options().advisory_lock_id_value()).await?;

        let outcome = async {
            let applied =
                load_applied_migrations(executor, self.options().migration_table()).await?;
            let plan = self.plan_rollback(&applied, steps)?;
            execute_plan(executor, self.options().migration_table(), &plan).await?;
            Ok(plan)
        }
        .await;

        let unlock = release_advisory_lock(executor, self.options().advisory_lock_id_value()).await;

        finish_locked_operation(outcome, unlock)
    }
}

#[cfg(not(loom))]
fn finish_locked_operation<T>(
    outcome: crate::Result<T>,
    unlock: crate::Result<()>,
) -> crate::Result<T> {
    match (outcome, unlock) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(err), Ok(()) | Err(_)) | (Ok(_), Err(err)) => Err(err),
    }
}

#[cfg(not(loom))]
async fn ensure_state_table<R>(executor: &R, table: &MigrationTable) -> crate::Result<()>
where
    R: MigrationExecutor,
{
    executor
        .simple_query_raw(&table.create_if_missing_sql())
        .await
        .map(|_| ())
}

#[cfg(not(loom))]
async fn load_applied_migrations<R>(
    executor: &R,
    table: &MigrationTable,
) -> crate::Result<Vec<AppliedMigration>>
where
    R: MigrationExecutor,
{
    let sql = format!(
        "SELECT version::text, \
                name, \
                up_checksum, \
                down_checksum, \
                up_transaction_mode, \
                down_transaction_mode, \
                EXTRACT(EPOCH FROM applied_at)::text \
         FROM {} \
         ORDER BY version ASC",
        table.qualified_name()
    );
    let result_sets = executor.simple_query_raw(&sql).await?;
    let Some(rows) = result_sets.into_iter().next() else {
        return Ok(Vec::new());
    };

    rows.iter().map(|row| parse_applied_row(row)).collect()
}

#[cfg(not(loom))]
fn parse_applied_row(row: &[Option<Bytes>]) -> crate::Result<AppliedMigration> {
    if row.len() != 7 {
        return Err(MigrationError::InvalidAppliedMigrationRow {
            reason: format!("expected 7 columns, found {}", row.len()),
        }
        .into());
    }

    let version = parse_row_value::<u64>(row, 0, "version")?;
    let name = parse_required_text(row, 1, "name")?;
    let id = MigrationId::new(version, name.to_string())?;
    let up_checksum = MigrationChecksum::parse(parse_required_text(row, 2, "up_checksum")?)?;
    let down_checksum = MigrationChecksum::parse(parse_required_text(row, 3, "down_checksum")?)?;
    let up_transaction_mode = parse_transaction_mode(row, 4, "up_transaction_mode")?;
    let down_transaction_mode = parse_transaction_mode(row, 5, "down_transaction_mode")?;
    let applied_at = parse_epoch_system_time(parse_required_text(row, 6, "applied_at")?)?;

    Ok(AppliedMigration::new(
        id,
        up_checksum,
        down_checksum,
        up_transaction_mode,
        down_transaction_mode,
        applied_at,
    ))
}

#[cfg(not(loom))]
async fn execute_plan<R>(
    executor: &R,
    table: &MigrationTable,
    plan: &MigrationPlan,
) -> crate::Result<()>
where
    R: MigrationExecutor,
{
    for step in plan.steps() {
        match step {
            MigrationPlanStep::Apply { pair } => {
                let state_sql = insert_applied_migration_sql(table, pair);
                execute_script_and_state_change(
                    executor,
                    pair.up().contents(),
                    pair.up().metadata().transaction_mode(),
                    state_sql,
                )
                .await?;
            }
            MigrationPlanStep::Rollback { pair, .. } => {
                let state_sql = delete_applied_migration_sql(table, pair.id().version());
                execute_script_and_state_change(
                    executor,
                    pair.down().contents(),
                    pair.down().metadata().transaction_mode(),
                    state_sql,
                )
                .await?;
            }
        }
    }

    Ok(())
}

#[cfg(not(loom))]
async fn execute_script_and_state_change<R>(
    executor: &R,
    script_sql: &str,
    transaction_mode: MigrationTransactionMode,
    state_change_sql: String,
) -> crate::Result<()>
where
    R: MigrationExecutor,
{
    match transaction_mode {
        MigrationTransactionMode::Transactional => {
            executor
                .transactional_batch(&[script_sql.to_string(), state_change_sql])
                .await
        }
        MigrationTransactionMode::NonTransactional => {
            executor.simple_query_raw(script_sql).await?;
            executor.transactional_batch(&[state_change_sql]).await
        }
    }
}

#[cfg(not(loom))]
async fn acquire_advisory_lock<R>(executor: &R, lock_id: i64) -> crate::Result<()>
where
    R: MigrationExecutor,
{
    executor
        .simple_query_raw(&format!("SELECT pg_advisory_lock({lock_id})"))
        .await
        .map(|_| ())
}

#[cfg(not(loom))]
async fn release_advisory_lock<R>(executor: &R, lock_id: i64) -> crate::Result<()>
where
    R: MigrationExecutor,
{
    let result_sets = executor
        .simple_query_raw(&format!("SELECT pg_advisory_unlock({lock_id})"))
        .await?;
    let Some(rows) = result_sets.into_iter().next() else {
        return Err(MigrationError::InvalidAppliedMigrationRow {
            reason: "advisory unlock did not return a result row".to_string(),
        }
        .into());
    };
    let Some(row) = rows.first() else {
        return Err(MigrationError::InvalidAppliedMigrationRow {
            reason: "advisory unlock returned an empty result set".to_string(),
        }
        .into());
    };
    let unlocked = parse_required_text(row, 0, "pg_advisory_unlock")?;
    if unlocked == "t" {
        Ok(())
    } else {
        Err(MigrationError::InvalidAppliedMigrationRow {
            reason: format!("advisory unlock returned {unlocked:?} instead of \"t\""),
        }
        .into())
    }
}

#[cfg(not(loom))]
fn insert_applied_migration_sql(table: &MigrationTable, pair: &super::MigrationPair) -> String {
    format!(
        "INSERT INTO {} \
         (version, name, up_checksum, down_checksum, up_transaction_mode, down_transaction_mode) \
         VALUES ({}, '{}', '{}', '{}', '{}', '{}')",
        table.qualified_name(),
        pair.id().version(),
        escape_sql_literal(pair.id().name()),
        pair.up().checksum(),
        pair.down().checksum(),
        pair.up().metadata().transaction_mode().as_str(),
        pair.down().metadata().transaction_mode().as_str(),
    )
}

#[cfg(not(loom))]
fn delete_applied_migration_sql(table: &MigrationTable, version: u64) -> String {
    format!(
        "DELETE FROM {} WHERE version = {}",
        table.qualified_name(),
        version
    )
}

#[cfg(not(loom))]
fn parse_transaction_mode(
    row: &[Option<Bytes>],
    index: usize,
    column: &str,
) -> crate::Result<MigrationTransactionMode> {
    MigrationTransactionMode::parse_directive(parse_required_text(row, index, column)?, column)
        .map_err(Into::into)
}

#[cfg(not(loom))]
fn parse_row_value<T>(row: &[Option<Bytes>], index: usize, column: &str) -> crate::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    parse_required_text(row, index, column)?
        .parse::<T>()
        .map_err(|err| {
            MigrationError::InvalidAppliedMigrationRow {
                reason: format!("could not parse {column}: {err}"),
            }
            .into()
        })
}

#[cfg(not(loom))]
fn parse_required_text<'a>(
    row: &'a [Option<Bytes>],
    index: usize,
    column: &str,
) -> crate::Result<&'a str> {
    let Some(cell) = row.get(index) else {
        return Err(MigrationError::InvalidAppliedMigrationRow {
            reason: format!("missing {column} column at index {index}"),
        }
        .into());
    };
    let Some(cell) = cell.as_ref() else {
        return Err(MigrationError::InvalidAppliedMigrationRow {
            reason: format!("{column} cannot be NULL"),
        }
        .into());
    };
    std::str::from_utf8(cell).map_err(|err| {
        MigrationError::InvalidAppliedMigrationRow {
            reason: format!("{column} was not valid UTF-8: {err}"),
        }
        .into()
    })
}

#[cfg(not(loom))]
fn parse_epoch_system_time(epoch: &str) -> crate::Result<SystemTime> {
    let seconds =
        epoch
            .parse::<f64>()
            .map_err(|err| MigrationError::InvalidAppliedMigrationRow {
                reason: format!("could not parse applied_at epoch seconds: {err}"),
            })?;
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(MigrationError::InvalidAppliedMigrationRow {
            reason: format!(
                "applied_at epoch seconds must be a finite, non-negative value: {epoch}"
            ),
        }
        .into());
    }
    Ok(UNIX_EPOCH + Duration::from_secs_f64(seconds))
}

#[cfg(not(loom))]
fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}
