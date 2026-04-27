use std::collections::{btree_map::Entry, BTreeMap};

use super::{
    AppliedMigration, MigrationAsset, MigrationError, MigrationId, MigrationKind, MigrationPair,
    MigrationScript, MigrationSource,
};

/// Fully-discovered on-disk migrations in validated version order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationCatalog {
    pairs: Vec<MigrationPair>,
}

impl MigrationCatalog {
    /// Build a validated catalog from raw source assets.
    pub fn from_assets(assets: Vec<MigrationAsset>) -> Result<Self, MigrationError> {
        let mut builders = BTreeMap::<MigrationId, PairBuilder>::new();
        let mut versions = BTreeMap::<u64, String>::new();

        for asset in assets {
            let source_path = asset.path().to_path_buf();
            let script = asset.parse()?;
            let id = script.id().clone();

            match versions.entry(id.version()) {
                Entry::Vacant(slot) => {
                    slot.insert(id.name().to_string());
                }
                Entry::Occupied(existing) if existing.get() != id.name() => {
                    return Err(MigrationError::ConflictingMigrationVersion {
                        version: id.version(),
                        existing_name: existing.get().clone(),
                        conflicting_name: id.name().to_string(),
                    });
                }
                Entry::Occupied(_) => {}
            }

            builders
                .entry(id.clone())
                .or_insert_with(|| PairBuilder::new(id))
                .insert(script, source_path)?;
        }

        let pairs = builders
            .into_values()
            .map(PairBuilder::finish)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { pairs })
    }

    /// Load and validate migrations from an arbitrary source.
    pub fn from_source<S: MigrationSource>(source: &S) -> Result<Self, MigrationError> {
        Self::from_assets(source.load()?)
    }

    /// Borrow migrations in deterministic version order.
    #[must_use]
    pub fn pairs(&self) -> &[MigrationPair] {
        &self.pairs
    }

    /// Compute migration status for all discovered migrations.
    pub fn status(&self, applied: &[AppliedMigration]) -> Result<MigrationStatus, MigrationError> {
        let applied_prefix = self.reconcile(applied)?;
        let mut entries = Vec::with_capacity(self.pairs.len());

        for (index, pair) in self.pairs.iter().enumerate() {
            let state = if let Some(applied) = applied_prefix.get(index) {
                MigrationStatusState::Applied {
                    applied: (*applied).clone(),
                }
            } else {
                MigrationStatusState::Pending
            };
            entries.push(MigrationStatusEntry {
                pair: pair.clone(),
                state,
            });
        }

        Ok(MigrationStatus {
            entries,
            applied_count: applied.len(),
        })
    }

    /// Build the dry-run apply plan needed to reach the latest on-disk migration.
    pub fn plan_apply(
        &self,
        applied: &[AppliedMigration],
    ) -> Result<MigrationPlan, MigrationError> {
        let applied_prefix = self.reconcile(applied)?;
        let steps = self.pairs[applied_prefix.len()..]
            .iter()
            .cloned()
            .map(|pair| MigrationPlanStep::Apply { pair })
            .collect();

        Ok(MigrationPlan {
            direction: MigrationPlanDirection::Apply,
            steps,
        })
    }

    /// Build the dry-run rollback plan for the last `steps` applied migrations.
    pub fn plan_rollback(
        &self,
        applied: &[AppliedMigration],
        steps: usize,
    ) -> Result<MigrationPlan, MigrationError> {
        let applied_prefix = self.reconcile(applied)?;
        let steps = applied_prefix
            .iter()
            .enumerate()
            .rev()
            .take(steps)
            .map(|(index, applied)| {
                let pair = self.pairs[index].clone();
                MigrationPlanStep::Rollback {
                    pair,
                    applied: (*applied).clone(),
                }
            })
            .collect();

        Ok(MigrationPlan {
            direction: MigrationPlanDirection::Rollback,
            steps,
        })
    }

    fn reconcile<'a>(
        &'a self,
        applied: &'a [AppliedMigration],
    ) -> Result<&'a [AppliedMigration], MigrationError> {
        let mut previous: Option<&MigrationId> = None;

        for (index, applied_migration) in applied.iter().enumerate() {
            let current = applied_migration.id();
            if let Some(previous) = previous {
                if current <= previous {
                    return Err(MigrationError::AppliedMigrationOrderMismatch {
                        expected: format!("migration after {previous}"),
                        actual: current.to_string(),
                    });
                }
            }
            previous = Some(current);

            let Some(source_pair) = self.pairs.get(index) else {
                return Err(MigrationError::AppliedMigrationMissing {
                    id: current.to_string(),
                });
            };

            if source_pair.id() != current {
                return Err(MigrationError::AppliedMigrationOrderMismatch {
                    expected: source_pair.id().to_string(),
                    actual: current.to_string(),
                });
            }

            verify_drift(applied_migration, source_pair)?;
        }

        Ok(applied)
    }
}

/// Migration status for every discovered source migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStatus {
    entries: Vec<MigrationStatusEntry>,
    applied_count: usize,
}

impl MigrationStatus {
    /// Borrow status entries in source order.
    #[must_use]
    pub fn entries(&self) -> &[MigrationStatusEntry] {
        &self.entries
    }

    /// Number of migrations already applied.
    #[must_use]
    pub const fn applied_count(&self) -> usize {
        self.applied_count
    }

    /// Number of migrations still pending.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.entries.len() - self.applied_count
    }
}

/// One source migration plus its current status against the migration table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStatusEntry {
    pair: MigrationPair,
    state: MigrationStatusState,
}

impl MigrationStatusEntry {
    /// Borrow the migration pair.
    #[must_use]
    pub fn pair(&self) -> &MigrationPair {
        &self.pair
    }

    /// Borrow the computed status.
    #[must_use]
    pub const fn state(&self) -> &MigrationStatusState {
        &self.state
    }
}

/// Whether a source migration is already applied or still pending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationStatusState {
    /// The migration exists in the state table and matches the source files.
    Applied {
        /// State-table row for this migration.
        applied: AppliedMigration,
    },
    /// The migration exists on disk but not in the applied prefix.
    Pending,
}

/// A deterministic dry-run plan for either applying or rolling back migrations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    direction: MigrationPlanDirection,
    steps: Vec<MigrationPlanStep>,
}

impl MigrationPlan {
    /// Which kind of plan this is.
    #[must_use]
    pub const fn direction(&self) -> MigrationPlanDirection {
        self.direction
    }

    /// Borrow the planned steps in execution order.
    #[must_use]
    pub fn steps(&self) -> &[MigrationPlanStep] {
        &self.steps
    }

    /// Whether this plan would do nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// Whether the plan applies pending migrations or rolls back applied ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationPlanDirection {
    /// Apply `up` scripts in ascending version order.
    Apply,
    /// Roll back `down` scripts in descending version order.
    Rollback,
}

/// One dry-run apply or rollback step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationPlanStep {
    /// Apply this migration's `up` file.
    Apply {
        /// The source migration pair that would be executed.
        pair: MigrationPair,
    },
    /// Roll back this migration's `down` file.
    Rollback {
        /// The source migration pair that would be executed.
        pair: MigrationPair,
        /// The matching applied migration row being reversed.
        applied: AppliedMigration,
    },
}

impl MigrationPlanStep {
    /// Borrow the underlying migration pair.
    #[must_use]
    pub fn pair(&self) -> &MigrationPair {
        match self {
            Self::Apply { pair } | Self::Rollback { pair, .. } => pair,
        }
    }

    /// Which script direction this step would execute.
    #[must_use]
    pub const fn kind(&self) -> MigrationKind {
        match self {
            Self::Apply { .. } => MigrationKind::Up,
            Self::Rollback { .. } => MigrationKind::Down,
        }
    }

    /// Borrow the applied state row when planning a rollback.
    #[must_use]
    pub const fn applied(&self) -> Option<&AppliedMigration> {
        match self {
            Self::Apply { .. } => None,
            Self::Rollback { applied, .. } => Some(applied),
        }
    }
}

#[derive(Debug)]
struct PairBuilder {
    id: MigrationId,
    up: Option<(MigrationScript, std::path::PathBuf)>,
    down: Option<(MigrationScript, std::path::PathBuf)>,
}

impl PairBuilder {
    fn new(id: MigrationId) -> Self {
        Self {
            id,
            up: None,
            down: None,
        }
    }

    fn insert(
        &mut self,
        script: MigrationScript,
        source_path: std::path::PathBuf,
    ) -> Result<(), MigrationError> {
        let (slot, kind) = match script.kind() {
            MigrationKind::Up => (&mut self.up, MigrationKind::Up),
            MigrationKind::Down => (&mut self.down, MigrationKind::Down),
        };

        if let Some((_, existing_path)) = slot {
            return Err(MigrationError::DuplicateMigrationScript {
                id: self.id.to_string(),
                kind: kind.to_string(),
                first: existing_path.clone(),
                second: source_path,
            });
        }

        *slot = Some((script, source_path));
        Ok(())
    }

    fn finish(self) -> Result<MigrationPair, MigrationError> {
        let Some((up, _)) = self.up else {
            return Err(MigrationError::MissingMigrationScript {
                id: self.id.to_string(),
                missing: MigrationKind::Up.to_string(),
            });
        };
        let Some((down, _)) = self.down else {
            return Err(MigrationError::MissingMigrationScript {
                id: self.id.to_string(),
                missing: MigrationKind::Down.to_string(),
            });
        };
        MigrationPair::new(up, down)
    }
}

fn verify_drift(applied: &AppliedMigration, pair: &MigrationPair) -> Result<(), MigrationError> {
    let id = applied.id().to_string();

    if applied.up_checksum() != pair.up().checksum() {
        return Err(MigrationError::DriftDetected {
            id,
            reason: format!(
                "up checksum changed from {} to {}",
                applied.up_checksum(),
                pair.up().checksum()
            ),
        });
    }

    if applied.down_checksum() != pair.down().checksum() {
        return Err(MigrationError::DriftDetected {
            id,
            reason: format!(
                "down checksum changed from {} to {}",
                applied.down_checksum(),
                pair.down().checksum()
            ),
        });
    }

    if applied.up_transaction_mode() != pair.up().metadata().transaction_mode() {
        return Err(MigrationError::DriftDetected {
            id,
            reason: format!(
                "up transaction mode changed from {} to {}",
                applied.up_transaction_mode().as_str(),
                pair.up().metadata().transaction_mode().as_str()
            ),
        });
    }

    if applied.down_transaction_mode() != pair.down().metadata().transaction_mode() {
        return Err(MigrationError::DriftDetected {
            id,
            reason: format!(
                "down transaction mode changed from {} to {}",
                applied.down_transaction_mode().as_str(),
                pair.down().metadata().transaction_mode().as_str()
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::{
        AppliedMigration, MigrationAsset, MigrationCatalog, MigrationError, MigrationKind,
        MigrationPlanDirection, MigrationPlanStep, MigrationStatusState,
    };
    use crate::migration::{MigrationChecksum, MigrationId, MigrationTransactionMode};

    #[test]
    fn catalog_pairs_and_orders_migrations() {
        let catalog = MigrationCatalog::from_assets(vec![
            MigrationAsset::new("2__second.down.sql", "DROP TABLE second;"),
            MigrationAsset::new("1__first.down.sql", "DROP TABLE first;"),
            MigrationAsset::new("2__second.up.sql", "CREATE TABLE second(id int);"),
            MigrationAsset::new("1__first.up.sql", "CREATE TABLE first(id int);"),
        ])
        .unwrap();

        let ids = catalog
            .pairs()
            .iter()
            .map(|pair| pair.id().to_string())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["1__first".to_string(), "2__second".to_string()]);
    }

    #[test]
    fn catalog_rejects_missing_pair() {
        let err = MigrationCatalog::from_assets(vec![MigrationAsset::new(
            "1__first.up.sql",
            "CREATE TABLE first(id int);",
        )])
        .unwrap_err();

        assert!(matches!(err, MigrationError::MissingMigrationScript { .. }));
    }

    #[test]
    fn catalog_rejects_conflicting_versions() {
        let err = MigrationCatalog::from_assets(vec![
            MigrationAsset::new("1__first.up.sql", "SELECT 1;"),
            MigrationAsset::new("1__first.down.sql", "SELECT 1;"),
            MigrationAsset::new("1__other.up.sql", "SELECT 2;"),
            MigrationAsset::new("1__other.down.sql", "SELECT 2;"),
        ])
        .unwrap_err();

        assert!(matches!(
            err,
            MigrationError::ConflictingMigrationVersion { version: 1, .. }
        ));
    }

    #[test]
    fn status_marks_applied_and_pending_migrations() {
        let catalog = fixture_catalog();
        let applied = vec![applied_migration(
            1,
            "first",
            "CREATE TABLE first(id int);",
            "DROP TABLE first;",
            MigrationTransactionMode::Transactional,
            MigrationTransactionMode::Transactional,
        )];

        let status = catalog.status(&applied).unwrap();
        assert_eq!(status.applied_count(), 1);
        assert_eq!(status.pending_count(), 1);
        assert!(matches!(
            status.entries()[0].state(),
            MigrationStatusState::Applied { .. }
        ));
        assert!(matches!(
            status.entries()[1].state(),
            MigrationStatusState::Pending
        ));
    }

    #[test]
    fn plan_apply_returns_pending_tail() {
        let catalog = fixture_catalog();
        let applied = vec![applied_migration(
            1,
            "first",
            "CREATE TABLE first(id int);",
            "DROP TABLE first;",
            MigrationTransactionMode::Transactional,
            MigrationTransactionMode::Transactional,
        )];

        let plan = catalog.plan_apply(&applied).unwrap();
        assert_eq!(plan.direction(), MigrationPlanDirection::Apply);
        assert_eq!(plan.steps().len(), 1);
        assert_eq!(plan.steps()[0].pair().id().to_string(), "2__second");
        assert_eq!(plan.steps()[0].kind(), MigrationKind::Up);
    }

    #[test]
    fn plan_rollback_returns_reverse_applied_prefix() {
        let catalog = fixture_catalog();
        let applied = vec![
            applied_migration(
                1,
                "first",
                "CREATE TABLE first(id int);",
                "DROP TABLE first;",
                MigrationTransactionMode::Transactional,
                MigrationTransactionMode::Transactional,
            ),
            applied_migration(
                2,
                "second",
                "--! babar:transaction = none\nCREATE INDEX CONCURRENTLY second_idx ON second(id);",
                "DROP INDEX second_idx;",
                MigrationTransactionMode::NonTransactional,
                MigrationTransactionMode::Transactional,
            ),
        ];

        let plan = catalog.plan_rollback(&applied, 2).unwrap();
        assert_eq!(plan.direction(), MigrationPlanDirection::Rollback);
        assert_eq!(plan.steps().len(), 2);
        assert_eq!(plan.steps()[0].pair().id().to_string(), "2__second");
        assert_eq!(plan.steps()[1].pair().id().to_string(), "1__first");
        assert!(matches!(
            plan.steps()[0],
            MigrationPlanStep::Rollback { .. }
        ));
    }

    #[test]
    fn drift_detection_rejects_checksum_changes() {
        let catalog = fixture_catalog();
        let applied = vec![applied_migration(
            1,
            "first",
            "CREATE TABLE first(id bigint);",
            "DROP TABLE first;",
            MigrationTransactionMode::Transactional,
            MigrationTransactionMode::Transactional,
        )];

        let err = catalog.status(&applied).unwrap_err();
        assert!(matches!(err, MigrationError::DriftDetected { .. }));
        assert!(err.to_string().contains("checksum"));
    }

    #[test]
    fn drift_detection_rejects_transaction_mode_changes() {
        let catalog = fixture_catalog();
        let applied = vec![
            applied_migration(
                1,
                "first",
                "CREATE TABLE first(id int);",
                "DROP TABLE first;",
                MigrationTransactionMode::Transactional,
                MigrationTransactionMode::Transactional,
            ),
            applied_migration(
                2,
                "second",
                "--! babar:transaction = none\nCREATE INDEX CONCURRENTLY second_idx ON second(id);",
                "DROP INDEX second_idx;",
                MigrationTransactionMode::Transactional,
                MigrationTransactionMode::Transactional,
            ),
        ];

        let err = catalog.plan_apply(&applied).unwrap_err();
        assert!(matches!(err, MigrationError::DriftDetected { .. }));
        assert!(err.to_string().contains("transaction mode"));
    }

    #[test]
    fn applied_history_must_match_source_prefix() {
        let catalog = fixture_catalog();
        let applied = vec![applied_migration(
            2,
            "second",
            "--! babar:transaction = none\nCREATE INDEX CONCURRENTLY second_idx ON second(id);",
            "DROP INDEX second_idx;",
            MigrationTransactionMode::NonTransactional,
            MigrationTransactionMode::Transactional,
        )];

        let err = catalog.plan_apply(&applied).unwrap_err();
        assert!(matches!(
            err,
            MigrationError::AppliedMigrationOrderMismatch { .. }
        ));
    }

    fn fixture_catalog() -> MigrationCatalog {
        MigrationCatalog::from_assets(vec![
            MigrationAsset::new("2__second.down.sql", "DROP INDEX second_idx;"),
            MigrationAsset::new(
                "2__second.up.sql",
                "--! babar:transaction = none\nCREATE INDEX CONCURRENTLY second_idx ON second(id);",
            ),
            MigrationAsset::new("1__first.down.sql", "DROP TABLE first;"),
            MigrationAsset::new("1__first.up.sql", "CREATE TABLE first(id int);"),
        ])
        .unwrap()
    }

    fn applied_migration(
        version: u64,
        name: &str,
        up_sql: &str,
        down_sql: &str,
        up_mode: MigrationTransactionMode,
        down_mode: MigrationTransactionMode,
    ) -> AppliedMigration {
        AppliedMigration::new(
            MigrationId::new(version, name).unwrap(),
            MigrationChecksum::of_contents(up_sql),
            MigrationChecksum::of_contents(down_sql),
            up_mode,
            down_mode,
            SystemTime::UNIX_EPOCH,
        )
    }
}
