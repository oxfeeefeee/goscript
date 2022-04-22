use quote::quote;
use syn::{parse_macro_input, DeriveInput};

pub fn derive_unsafe_ptr_implement(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics UnsafePtr for #name #ty_generics #where_clause {
            fn as_any(&self) -> &dyn Any {
                self
            }
        }
    };
    proc_macro::TokenStream::from(expanded)
}
