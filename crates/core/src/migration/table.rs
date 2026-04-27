use super::MigrationError;

/// Default schema that stores babar's migration state table.
pub const DEFAULT_MIGRATION_SCHEMA: &str = "public";

/// Default table name that stores applied migrations.
pub const DEFAULT_MIGRATION_TABLE: &str = "babar_schema_migrations";

/// Default advisory lock identifier reserved for the migration runner.
pub const DEFAULT_MIGRATION_ADVISORY_LOCK_ID: i64 = 0x0062_6162_6172;

/// Configuration for babar's migration state table.
///
/// The state table schema is:
///
/// ```sql
/// CREATE TABLE IF NOT EXISTS public.babar_schema_migrations (
///     version bigint PRIMARY KEY,
///     name text NOT NULL,
///     up_checksum text NOT NULL,
///     down_checksum text NOT NULL,
///     up_transaction_mode text NOT NULL,
///     down_transaction_mode text NOT NULL,
///     applied_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
/// );
/// ```
///
/// `up_transaction_mode` and `down_transaction_mode` store `transactional` or
/// `non_transactional`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationTable {
    schema: String,
    name: String,
}

impl Default for MigrationTable {
    fn default() -> Self {
        Self {
            schema: DEFAULT_MIGRATION_SCHEMA.to_string(),
            name: DEFAULT_MIGRATION_TABLE.to_string(),
        }
    }
}

impl MigrationTable {
    /// Build a migration table configuration.
    pub fn new(schema: impl Into<String>, name: impl Into<String>) -> Result<Self, MigrationError> {
        let schema = schema.into();
        let name = name.into();
        validate_part(&schema, &name, "schema")?;
        validate_part(&schema, &name, "table")?;
        Ok(Self { schema, name })
    }

    /// Schema that holds the migration table.
    #[must_use]
    pub fn schema(&self) -> &str {
        &self.schema
    }

    /// Table name used for migration state.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Fully-qualified, safely quoted table name.
    #[must_use]
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", quote_ident(&self.schema), quote_ident(&self.name))
    }

    /// SQL that creates babar's migration state table if it does not exist.
    #[must_use]
    pub fn create_if_missing_sql(&self) -> String {
        format!(
            "CREATE TABLE IF NOT EXISTS {} (\
             version bigint PRIMARY KEY, \
             name text NOT NULL, \
             up_checksum text NOT NULL, \
             down_checksum text NOT NULL, \
             up_transaction_mode text NOT NULL CHECK (up_transaction_mode IN ('transactional', 'non_transactional')), \
             down_transaction_mode text NOT NULL CHECK (down_transaction_mode IN ('transactional', 'non_transactional')), \
             applied_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP\
             )",
            self.qualified_name()
        )
    }
}

fn validate_part(schema: &str, name: &str, part: &str) -> Result<(), MigrationError> {
    let value = match part {
        "schema" => schema,
        _ => name,
    };

    if value.trim().is_empty() || value.contains('\0') {
        return Err(MigrationError::InvalidTable {
            qualified_name: format!("{schema}.{name}"),
            reason: format!("{part} must be non-empty and cannot contain NUL"),
        });
    }
    Ok(())
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
