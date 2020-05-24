#![allow(dead_code)]

use super::objects::{PackageKey, ScopeKey, TypeKey};

/// Universe sets up the universe scope and the unsafe package.
///
pub struct Universe {
    scope: ScopeKey,
    unsaf: PackageKey,
    types: Vec<TypeKey>,
}

impl Universe {
    pub fn new(scope: ScopeKey, unsaf: PackageKey) -> Universe {
        let types = Universe::init_types();
        Universe {
            scope: scope,
            unsaf: unsaf,
            types: types,
        }
    }

    pub fn scope(&self) -> &ScopeKey {
        &self.scope
    }

    fn init_types() -> Vec<TypeKey> {
        vec![]
    }
}
