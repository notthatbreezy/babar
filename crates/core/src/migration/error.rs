use std::path::PathBuf;

/// Errors returned while parsing or configuring migrations.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum MigrationError {
    /// A migration file name did not match babar's expected grammar.
    #[error("invalid migration file name {file_name:?}; expected {expected}")]
    InvalidFileName {
        /// The file name that failed validation.
        file_name: String,
        /// A human-readable summary of the expected grammar.
        expected: &'static str,
    },

    /// A migration version segment was invalid.
    #[error("invalid migration version {version:?}; expected an unsigned integer")]
    InvalidVersion {
        /// The raw version string from the file name.
        version: String,
    },

    /// A migration name segment was invalid.
    #[error("invalid migration name {name:?}; expected lowercase snake_case")]
    InvalidName {
        /// The raw migration name from the file name.
        name: String,
    },

    /// A migration metadata directive was malformed.
    #[error("invalid migration directive {directive:?}: {reason}")]
    InvalidDirective {
        /// The raw directive line.
        directive: String,
        /// Why the directive was rejected.
        reason: String,
    },

    /// A migration pair did not contain matching `up` / `down` files.
    #[error("migration pair mismatch: {up} does not match {down}")]
    PairMismatch {
        /// The `up` file involved in the mismatch.
        up: String,
        /// The `down` file involved in the mismatch.
        down: String,
    },

    /// A stored checksum string was malformed.
    #[error("invalid migration checksum {checksum:?}; expected 64 lowercase hex characters")]
    InvalidChecksum {
        /// The malformed checksum value.
        checksum: String,
    },

    /// A migration source configuration was invalid.
    #[error("invalid migration source path {path:?}: {reason}")]
    InvalidSourcePath {
        /// The path that failed validation.
        path: PathBuf,
        /// Why the path was rejected.
        reason: String,
    },

    /// Two source files claimed the same migration id and direction.
    #[error("duplicate migration script for {id} ({kind}): {first:?} and {second:?}")]
    DuplicateMigrationScript {
        /// The duplicated logical migration id.
        id: String,
        /// Which script direction was duplicated.
        kind: String,
        /// The first source path that claimed this slot.
        first: PathBuf,
        /// The second source path that claimed this slot.
        second: PathBuf,
    },

    /// Two different migration names claimed the same version.
    #[error(
        "conflicting migration version {version}: {existing_name:?} conflicts with {conflicting_name:?}"
    )]
    ConflictingMigrationVersion {
        /// The duplicated numeric version.
        version: u64,
        /// The first name seen for this version.
        existing_name: String,
        /// The later conflicting name.
        conflicting_name: String,
    },

    /// A migration was missing one side of the required up/down pair.
    #[error("missing {missing} migration script for {id}")]
    MissingMigrationScript {
        /// The logical migration id missing a file.
        id: String,
        /// Which file is missing.
        missing: String,
    },

    /// The applied migration history referenced a migration that is not on disk.
    #[error("applied migration {id} is not present in the migration source")]
    AppliedMigrationMissing {
        /// The missing migration id.
        id: String,
    },

    /// The applied migration history did not match the source prefix ordering.
    #[error("applied migration order mismatch: expected {expected}, found {actual}")]
    AppliedMigrationOrderMismatch {
        /// The migration id expected at this point from the source.
        expected: String,
        /// The migration id present in the applied history.
        actual: String,
    },

    /// An applied migration no longer matches the on-disk source files.
    #[error("migration drift detected for {id}: {reason}")]
    DriftDetected {
        /// The drifted migration id.
        id: String,
        /// Why the source and state table no longer match.
        reason: String,
    },

    /// A migration table configuration was invalid.
    #[error("invalid migration table {qualified_name:?}: {reason}")]
    InvalidTable {
        /// The fully-qualified table name, when available.
        qualified_name: String,
        /// Why the table configuration was rejected.
        reason: String,
    },

    /// One row read from the migration state table was malformed.
    #[error("invalid applied migration row: {reason}")]
    InvalidAppliedMigrationRow {
        /// Why the row could not be decoded into [`super::AppliedMigration`].
        reason: String,
    },
}
