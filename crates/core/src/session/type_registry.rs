//! Per-session resolution cache for dynamic PostgreSQL types.

use std::collections::HashMap;

use crate::types::{Oid, Type};

/// Session-local cache of dynamically resolved type OIDs.
#[derive(Debug, Default)]
pub(crate) struct TypeRegistry {
    resolved: HashMap<Type, Oid>,
}

impl TypeRegistry {
    /// Look up a previously resolved type OID.
    pub fn get(&self, ty: Type) -> Option<Oid> {
        self.resolved.get(&ty).copied()
    }

    /// Insert a resolved type OID.
    pub fn insert(&mut self, ty: Type, oid: Oid) {
        self.resolved.insert(ty, oid);
    }
}
