#[macro_use]
mod util;

mod assignment;
mod builtin;
mod call;
pub mod check;
mod decl;
mod expr;
mod interface;
mod resolver;
mod stmt;
mod typexpr;

pub use interface::{IfaceInfo, MethodInfo};
pub use resolver::DeclInfo;
