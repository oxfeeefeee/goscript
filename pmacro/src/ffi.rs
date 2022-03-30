use quote::quote;
use syn::{parse_macro_input, DeriveInput};

pub fn derive_ffi_implement(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);

    // Used in the quasi-quotation below as `#name`.
    let name = input.ident;

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        // The generated impl.
        impl #impl_generics Ffi for #name #ty_generics #where_clause {
            fn call(
                &self,
                ctx: &mut FfiCallCtx,
                args: Vec<GosValue>,
            ) -> Pin<Box<dyn Future<Output = goscript_vm::value::RuntimeResult<Vec<GosValue>>> + '_>> {
                self.dispatch(ctx, args)
            }
        }
    };

    // Hand the output tokens back to the compiler.
    proc_macro::TokenStream::from(expanded)
}
