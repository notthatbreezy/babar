use std::time::SystemTime;

use super::{MigrationChecksum, MigrationError, MigrationFilename, MigrationId, MigrationKind};

/// How babar should execute one migration script with respect to transactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MigrationTransactionMode {
    /// Wrap the script in an explicit transaction.
    #[default]
    Transactional,
    /// Execute the script outside an explicit transaction.
    NonTransactional,
}

impl MigrationTransactionMode {
    /// Stable string form stored in the migration state table.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transactional => "transactional",
            Self::NonTransactional => "non_transactional",
        }
    }

    pub(crate) fn parse_directive(value: &str, directive: &str) -> Result<Self, MigrationError> {
        match value {
            "transactional" => Ok(Self::Transactional),
            "none" | "non_transactional" => Ok(Self::NonTransactional),
            _ => Err(MigrationError::InvalidDirective {
                directive: directive.to_string(),
                reason: "supported values are transactional and none".to_string(),
            }),
        }
    }
}

/// Parsed metadata pragma lines from the top of one migration SQL file.
///
/// babar currently supports one directive:
///
/// ```sql
/// --! babar:transaction = none
/// ```
///
/// The directive must appear before the first non-blank SQL line. If omitted,
/// the script runs transactionally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MigrationScriptMetadata {
    transaction_mode: MigrationTransactionMode,
}

impl MigrationScriptMetadata {
    /// Parse metadata pragmas from the beginning of a migration SQL file.
    pub fn parse(contents: &str) -> Result<Self, MigrationError> {
        let mut metadata = Self::default();
        let mut saw_transaction = false;

        for raw_line in contents.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Some(directive) = trimmed.strip_prefix("--!") else {
                break;
            };
            let Some(directive) = directive.trim().strip_prefix("babar:") else {
                return Err(MigrationError::InvalidDirective {
                    directive: raw_line.to_string(),
                    reason: "expected `--! babar:<key> = <value>`".to_string(),
                });
            };
            let (key, value) =
                directive
                    .split_once('=')
                    .ok_or_else(|| MigrationError::InvalidDirective {
                        directive: raw_line.to_string(),
                        reason: "expected `key = value`".to_string(),
                    })?;

            let key = key.trim();
            let value = value.trim();

            match key {
                "transaction" => {
                    if saw_transaction {
                        return Err(MigrationError::InvalidDirective {
                            directive: raw_line.to_string(),
                            reason: "duplicate transaction directive".to_string(),
                        });
                    }
                    metadata.transaction_mode =
                        MigrationTransactionMode::parse_directive(value, raw_line)?;
                    saw_transaction = true;
                }
                _ => {
                    return Err(MigrationError::InvalidDirective {
                        directive: raw_line.to_string(),
                        reason: format!("unsupported directive key `{key}`"),
                    });
                }
            }
        }

        Ok(metadata)
    }

    /// Transaction mode declared for this file.
    #[must_use]
    pub const fn transaction_mode(&self) -> MigrationTransactionMode {
        self.transaction_mode
    }
}

/// One parsed migration SQL file, including metadata and checksum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationScript {
    file: MigrationFilename,
    metadata: MigrationScriptMetadata,
    contents: String,
    checksum: MigrationChecksum,
}

impl MigrationScript {
    /// Parse one migration script from an already-validated file name and its contents.
    pub fn new(
        file: MigrationFilename,
        contents: impl Into<String>,
    ) -> Result<Self, MigrationError> {
        let contents = contents.into();
        let metadata = MigrationScriptMetadata::parse(&contents)?;
        let checksum = MigrationChecksum::of_contents(&contents);
        Ok(Self {
            file,
            metadata,
            contents,
            checksum,
        })
    }

    /// The parsed file metadata for this script.
    #[must_use]
    pub fn file(&self) -> &MigrationFilename {
        &self.file
    }

    /// The logical migration identifier for this script.
    #[must_use]
    pub fn id(&self) -> &MigrationId {
        self.file.id()
    }

    /// Whether this is the `up` or `down` script.
    #[must_use]
    pub const fn kind(&self) -> MigrationKind {
        self.file.kind()
    }

    /// Parsed metadata directives for this script.
    #[must_use]
    pub const fn metadata(&self) -> &MigrationScriptMetadata {
        &self.metadata
    }

    /// Exact SQL file contents, including directive comments.
    #[must_use]
    pub fn contents(&self) -> &str {
        &self.contents
    }

    /// SHA-256 checksum of the exact file contents.
    #[must_use]
    pub const fn checksum(&self) -> MigrationChecksum {
        self.checksum
    }
}

/// A validated pair of `up` and `down` migration scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPair {
    id: MigrationId,
    up: MigrationScript,
    down: MigrationScript,
}

impl MigrationPair {
    /// Build one migration pair from matching `up` and `down` files.
    pub fn new(up: MigrationScript, down: MigrationScript) -> Result<Self, MigrationError> {
        if up.kind() != MigrationKind::Up
            || down.kind() != MigrationKind::Down
            || up.id() != down.id()
        {
            return Err(MigrationError::PairMismatch {
                up: up.file().to_string(),
                down: down.file().to_string(),
            });
        }

        Ok(Self {
            id: up.id().clone(),
            up,
            down,
        })
    }

    /// Logical migration identifier for both files.
    #[must_use]
    pub fn id(&self) -> &MigrationId {
        &self.id
    }

    /// The `up` script.
    #[must_use]
    pub fn up(&self) -> &MigrationScript {
        &self.up
    }

    /// The `down` script.
    #[must_use]
    pub fn down(&self) -> &MigrationScript {
        &self.down
    }
}

/// One applied migration row from babar's migration state table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMigration {
    id: MigrationId,
    up_checksum: MigrationChecksum,
    down_checksum: MigrationChecksum,
    up_transaction_mode: MigrationTransactionMode,
    down_transaction_mode: MigrationTransactionMode,
    applied_at: SystemTime,
}

impl AppliedMigration {
    /// Build an applied-migration record using the state table schema.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        id: MigrationId,
        up_checksum: MigrationChecksum,
        down_checksum: MigrationChecksum,
        up_transaction_mode: MigrationTransactionMode,
        down_transaction_mode: MigrationTransactionMode,
        applied_at: SystemTime,
    ) -> Self {
        Self {
            id,
            up_checksum,
            down_checksum,
            up_transaction_mode,
            down_transaction_mode,
            applied_at,
        }
    }

    /// Logical migration identifier.
    #[must_use]
    pub fn id(&self) -> &MigrationId {
        &self.id
    }

    /// Stored checksum for the `up` file.
    #[must_use]
    pub const fn up_checksum(&self) -> MigrationChecksum {
        self.up_checksum
    }

    /// Stored checksum for the `down` file.
    #[must_use]
    pub const fn down_checksum(&self) -> MigrationChecksum {
        self.down_checksum
    }

    /// Stored transaction mode for the `up` file.
    #[must_use]
    pub const fn up_transaction_mode(&self) -> MigrationTransactionMode {
        self.up_transaction_mode
    }

    /// Stored transaction mode for the `down` file.
    #[must_use]
    pub const fn down_transaction_mode(&self) -> MigrationTransactionMode {
        self.down_transaction_mode
    }

    /// When the migration was recorded as applied.
    #[must_use]
    pub const fn applied_at(&self) -> SystemTime {
        self.applied_at
    }
}
