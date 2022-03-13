pub use goscript_vm::ffi::{Ffi, FfiCallCtx, FfiCtorResult};

#[macro_export]
macro_rules! non_async_result {
    ($($x:expr),*) => {Box::pin(async move { Ok(vec![$($x),*]) })}
}
