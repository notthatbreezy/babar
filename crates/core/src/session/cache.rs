//! Per-session prepared statement cache.
//!
//! Keyed by `(sql_hash, param_oids)` so two queries that differ only in
//! their decoder (but share SQL + encoder) still share a prepared
//! statement server-side.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::protocol::backend::RowField;
use crate::types::Oid;

/// Cache key: hash of (SQL text, encoder OIDs).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    sql_hash: u64,
    param_oids: Vec<Oid>,
}

impl CacheKey {
    pub fn new(sql: &str, param_oids: &[Oid]) -> Self {
        let mut hasher = std::hash::DefaultHasher::new();
        sql.hash(&mut hasher);
        Self {
            sql_hash: hasher.finish(),
            param_oids: param_oids.to_vec(),
        }
    }
}

/// Metadata returned by `Describe Statement` and cached for the lifetime
/// of the prepared statement.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct CachedStatement {
    /// Server-assigned name for this prepared statement.
    pub name: String,
    /// Parameter OIDs the server inferred (from `ParameterDescription`).
    pub param_oids: Vec<u32>,
    /// Column metadata (from `RowDescription`), or empty if the statement
    /// returns no rows.
    pub row_fields: Vec<RowField>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    stmt: CachedStatement,
    handles: usize,
}

/// Per-session statement cache.
#[derive(Debug, Default)]
pub(crate) struct StatementCache {
    stmts: HashMap<CacheKey, CacheEntry>,
    counter: u64,
    retain_on_zero_handles: bool,
}

impl StatementCache {
    pub fn new(retain_on_zero_handles: bool) -> Self {
        Self {
            retain_on_zero_handles,
            ..Self::default()
        }
    }

    /// Look up a cached statement by key and acquire another live handle to it.
    pub fn checkout(&mut self, key: &CacheKey) -> Option<CachedStatement> {
        let entry = self.stmts.get_mut(key)?;
        entry.handles += 1;
        Some(entry.stmt.clone())
    }

    /// Insert a freshly prepared statement with one live handle.
    pub fn insert(&mut self, key: CacheKey, stmt: CachedStatement) {
        self.stmts.insert(key, CacheEntry { stmt, handles: 1 });
    }

    /// Release one prepared-statement handle. Returns the cached statement
    /// metadata only when the last live handle goes away and the server-side
    /// statement should be closed.
    pub fn release_handle(&mut self, key: &CacheKey) -> Option<CachedStatement> {
        let entry = self.stmts.get_mut(key)?;
        if entry.handles > 1 {
            entry.handles -= 1;
            return None;
        }
        if self.retain_on_zero_handles {
            entry.handles = 0;
            None
        } else {
            self.stmts.remove(key).map(|entry| entry.stmt)
        }
    }

    /// Generate a unique statement name for this session.
    pub fn next_name(&mut self) -> String {
        self.counter += 1;
        format!("_babar_{}", self.counter)
    }

    /// Number of cached statements.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.stmts.len()
    }
}
