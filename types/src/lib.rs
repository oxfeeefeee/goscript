/// Goscirpt's type checker
/// A lot of comments are directly taken from Go source file.
///
mod constant;

mod obj;
mod package;
mod scope;

#[macro_use]
mod typ;

mod lookup;
mod objects;
mod selection;

mod operand;

mod universe;

mod check;

mod display;
mod importer;
