use std::fs;
use std::path::{Path, PathBuf};

use super::{MigrationError, MigrationFilename, MigrationScript};

/// One raw migration asset loaded from a source before pairing and planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationAsset {
    path: PathBuf,
    contents: String,
}

impl MigrationAsset {
    /// Build one raw migration asset.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            contents: contents.into(),
        }
    }

    /// Source path for this asset.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Raw SQL file contents.
    #[must_use]
    pub fn contents(&self) -> &str {
        &self.contents
    }

    /// Parse this asset into a typed migration script.
    pub fn parse(&self) -> Result<MigrationScript, MigrationError> {
        let file_name = self
            .path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| MigrationError::InvalidSourcePath {
                path: self.path.clone(),
                reason: "path must end in a UTF-8 file name".to_string(),
            })?;

        let file = MigrationFilename::parse(file_name)?;
        MigrationScript::new(file, self.contents.clone())
    }
}

/// Shared abstraction used by both the library API and the CLI to provide SQL files.
pub trait MigrationSource {
    /// Load raw migration assets from this source.
    fn load(&self) -> Result<Vec<MigrationAsset>, MigrationError>;
}

/// A test-friendly in-memory migration source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryMigrationSource {
    assets: Vec<MigrationAsset>,
}

impl MemoryMigrationSource {
    /// Build a source from preloaded migration assets.
    #[must_use]
    pub fn new(assets: Vec<MigrationAsset>) -> Self {
        Self { assets }
    }

    /// Borrow the configured assets.
    #[must_use]
    pub fn assets(&self) -> &[MigrationAsset] {
        &self.assets
    }
}

impl MigrationSource for MemoryMigrationSource {
    fn load(&self) -> Result<Vec<MigrationAsset>, MigrationError> {
        Ok(self.assets.clone())
    }
}

/// A directory-backed migration source for the eventual filesystem discovery layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSystemMigrationSource {
    root: PathBuf,
}

impl FileSystemMigrationSource {
    /// Build a source rooted at one directory.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Borrow the configured root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl MigrationSource for FileSystemMigrationSource {
    fn load(&self) -> Result<Vec<MigrationAsset>, MigrationError> {
        let metadata =
            fs::metadata(&self.root).map_err(|error| MigrationError::InvalidSourcePath {
                path: self.root.clone(),
                reason: error.to_string(),
            })?;

        if !metadata.is_dir() {
            return Err(MigrationError::InvalidSourcePath {
                path: self.root.clone(),
                reason: "path must point to a directory".to_string(),
            });
        }

        let mut assets = Vec::new();
        let entries =
            fs::read_dir(&self.root).map_err(|error| MigrationError::InvalidSourcePath {
                path: self.root.clone(),
                reason: error.to_string(),
            })?;

        for entry in entries {
            let entry = entry.map_err(|error| MigrationError::InvalidSourcePath {
                path: self.root.clone(),
                reason: error.to_string(),
            })?;
            let path = entry.path();
            let metadata =
                fs::metadata(&path).map_err(|error| MigrationError::InvalidSourcePath {
                    path: path.clone(),
                    reason: error.to_string(),
                })?;

            if !metadata.is_file() {
                continue;
            }

            let Some(_) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
                return Err(MigrationError::InvalidSourcePath {
                    path,
                    reason: "path must end in a UTF-8 file name".to_string(),
                });
            };

            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("sql"))
            {
                continue;
            }

            let contents =
                fs::read_to_string(&path).map_err(|error| MigrationError::InvalidSourcePath {
                    path: path.clone(),
                    reason: error.to_string(),
                })?;
            assets.push(MigrationAsset::new(path, contents));
        }

        assets.sort_by(|left, right| left.path().cmp(right.path()));
        Ok(assets)
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{FileSystemMigrationSource, MigrationSource};

    #[test]
    fn filesystem_source_loads_sql_files_in_stable_order() {
        let dir = TestDir::new("load-order");
        dir.write("2__later.up.sql", "SELECT 2;");
        dir.write("1__first.up.sql", "SELECT 1;");
        dir.write("ignore.txt", "not a migration");

        let source = FileSystemMigrationSource::new(dir.path());
        let assets = source.load().unwrap();
        let file_names = assets
            .iter()
            .map(|asset| {
                asset
                    .path()
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            file_names,
            vec!["1__first.up.sql".to_string(), "2__later.up.sql".to_string()]
        );
    }

    #[test]
    fn filesystem_source_rejects_non_directory_root() {
        let dir = TestDir::new("invalid-root");
        let file = dir.path().join("1__first.up.sql");
        std::fs::write(&file, "SELECT 1;").unwrap();

        let err = FileSystemMigrationSource::new(&file).load().unwrap_err();
        assert!(err.to_string().contains("directory"));
    }

    #[derive(Debug)]
    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("test-artifacts")
                .join(format!("{label}-{unique}"));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write(&self, name: &str, contents: &str) {
            std::fs::write(self.path.join(name), contents).unwrap();
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
