use super::objects::{PackageKey, ScopeKey};

/// A Package describes a Go package.
pub struct Package {
    path: String,
    name: String,
    scope: ScopeKey,
    complete: bool,
    imports: Vec<PackageKey>,
    // scope lookup errors are silently dropped if package is fake (internal use only)
    fake: bool,
}
