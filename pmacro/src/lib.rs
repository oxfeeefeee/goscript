mod ffi;
mod ffi_impl;

#[proc_macro_derive(Ffi)]
pub fn derive_ffi(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    ffi::derive_ffi_implement(input)
}

#[proc_macro_attribute]
pub fn ffi_impl(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    ffi_impl::ffi_impl_implement(args, input)
}
