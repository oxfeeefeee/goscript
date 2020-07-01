#[macro_use]
mod util;

mod assignment;
mod call;
pub mod check;
mod decl;
mod expr;
mod interface;
mod resolver;
mod stmt;
mod typexpr;

pub use interface::IfaceInfo;
pub use resolver::DeclInfo;
