#[macro_use]
mod util;

mod assignment;
mod builtin;
mod call;
pub mod check;
mod conversion;
mod decl;
mod expr;
mod initorder;
mod interface;
mod label;
mod resolver;
mod returns;
mod stmt;
mod typexpr;

pub use check::Checker;
pub use interface::{IfaceInfo, MethodInfo};
pub use resolver::DeclInfo;
