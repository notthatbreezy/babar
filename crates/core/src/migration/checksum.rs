use std::fmt::{self, Write as _};

use sha2::{Digest as _, Sha256};

use super::MigrationError;

/// A SHA-256 checksum of one migration script's exact file contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MigrationChecksum([u8; 32]);

impl MigrationChecksum {
    /// Compute the checksum for raw SQL file contents.
    #[must_use]
    pub fn of_contents(contents: &str) -> Self {
        let digest = Sha256::digest(contents.as_bytes());
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(&digest);
        Self(bytes)
    }

    /// Parse a lowercase hexadecimal checksum from the migration state table.
    pub fn parse(hex: &str) -> Result<Self, MigrationError> {
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(MigrationError::InvalidChecksum {
                checksum: hex.to_string(),
            });
        }

        let mut bytes = [0_u8; 32];
        for (index, slot) in bytes.iter_mut().enumerate() {
            let offset = index * 2;
            *slot = u8::from_str_radix(&hex[offset..offset + 2], 16).map_err(|_| {
                MigrationError::InvalidChecksum {
                    checksum: hex.to_string(),
                }
            })?;
        }
        Ok(Self(bytes))
    }

    /// Render the checksum as lowercase hexadecimal.
    #[must_use]
    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            let _ = write!(output, "{byte:02x}");
        }
        output
    }

    /// Borrow the raw SHA-256 digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for MigrationChecksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}
