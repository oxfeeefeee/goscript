#[macro_use]
mod util;

mod assignment;
pub mod check;
mod decl;
mod expr;
mod interface;
mod resolver;
mod typexpr;

pub use resolver::DeclInfo;
