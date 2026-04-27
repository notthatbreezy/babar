use std::fmt;

use super::MigrationError;

const EXPECTED_FILE_NAME: &str = "<version>__<name>.<up|down>.sql";

/// A logical migration identifier shared by the paired `up` and `down` files.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MigrationId {
    version: u64,
    name: String,
}

impl MigrationId {
    /// Build a validated migration identifier.
    pub fn new(version: u64, name: impl Into<String>) -> Result<Self, MigrationError> {
        let name = name.into();
        validate_name(&name)?;
        Ok(Self { version, name })
    }

    /// Numeric migration version used for stable ordering.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Human-readable migration name from the file slug.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for MigrationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}__{}", self.version, self.name)
    }
}

/// The direction of one migration SQL file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MigrationKind {
    /// Apply this migration.
    Up,
    /// Roll back this migration.
    Down,
}

impl MigrationKind {
    /// File extension segment used by this direction.
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

impl fmt::Display for MigrationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.suffix())
    }
}

/// A parsed migration file name.
///
/// babar's migration file grammar is:
///
/// ```text
/// <version>__<name>.<up|down>.sql
/// ```
///
/// Examples:
///
/// - `1__bootstrap.up.sql`
/// - `20240623153000__create_users.down.sql`
///
/// The version is an unsigned integer. The name must be lowercase `snake_case`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MigrationFilename {
    id: MigrationId,
    kind: MigrationKind,
}

impl MigrationFilename {
    /// Parse one migration file name.
    pub fn parse(file_name: &str) -> Result<Self, MigrationError> {
        let stem =
            file_name
                .strip_suffix(".sql")
                .ok_or_else(|| MigrationError::InvalidFileName {
                    file_name: file_name.to_string(),
                    expected: EXPECTED_FILE_NAME,
                })?;

        let (base, kind) = if let Some(base) = stem.strip_suffix(".up") {
            (base, MigrationKind::Up)
        } else if let Some(base) = stem.strip_suffix(".down") {
            (base, MigrationKind::Down)
        } else {
            return Err(MigrationError::InvalidFileName {
                file_name: file_name.to_string(),
                expected: EXPECTED_FILE_NAME,
            });
        };

        let (version, name) =
            base.split_once("__")
                .ok_or_else(|| MigrationError::InvalidFileName {
                    file_name: file_name.to_string(),
                    expected: EXPECTED_FILE_NAME,
                })?;

        let version = version
            .parse::<u64>()
            .map_err(|_| MigrationError::InvalidVersion {
                version: version.to_string(),
            })?;

        Ok(Self {
            id: MigrationId::new(version, name.to_string())?,
            kind,
        })
    }

    /// The logical migration identifier without direction.
    #[must_use]
    pub fn id(&self) -> &MigrationId {
        &self.id
    }

    /// The direction (`up` or `down`) of this file.
    #[must_use]
    pub const fn kind(&self) -> MigrationKind {
        self.kind
    }

    /// Render this parsed file name back to babar's canonical format.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}.{}.sql", self.id, self.kind.suffix())
    }
}

impl fmt::Display for MigrationFilename {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.file_name())
    }
}

fn validate_name(name: &str) -> Result<(), MigrationError> {
    if name.is_empty()
        || name.starts_with('_')
        || name.ends_with('_')
        || name.contains("__")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(MigrationError::InvalidName {
            name: name.to_string(),
        });
    }

    Ok(())
}
