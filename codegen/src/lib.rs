mod branch;
mod emit;
mod interface;
mod package;
mod types;

pub mod codegen;
pub mod entry;
pub use entry::parse_check_gen;
pub use goscript_types::Config;
