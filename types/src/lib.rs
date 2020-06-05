/// Goscirpt's type checker
/// A lot of comments are directly taken from Go source file.
///
mod constant;

mod obj;
mod package;
mod scope;
mod typ;

mod lookup;
mod objects;
mod selection;

mod operand;

mod universe;

mod check;

mod importer;
