/// Goscirpt's type checker
/// A lot of comments are directly taken from Go source file.
///
mod constant;

mod obj;
mod package;
mod scope;

#[macro_use]
pub mod objects;

#[macro_use]
mod typ;

mod lookup;
mod operand;
mod selection;

mod universe;

mod display;

mod check;

mod importer;

pub use importer::{Config, ImportKey, Importer};
