/// Goscirpt's type checker
/// This is a port of the offical Go 1.12 type checker
/// A lot of comments are directly taken from Go source file.
///
mod constant;

mod obj;
mod package;
mod scope;

#[macro_use]
mod objects;

#[macro_use]
mod typ;

mod lookup;
mod operand;
mod selection;

mod universe;

mod display;

mod check;

mod importer;

pub use check::{DeclInfo, TypeInfo};
pub use importer::{Config, ImportKey, Importer};
pub use obj::EntityType;
pub use objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
pub use typ::BasicType;
