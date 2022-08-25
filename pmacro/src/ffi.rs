// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use quote::quote;
use syn::{parse_macro_input, DeriveInput};

pub fn derive_ffi_implement(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the input tokens into a syntax tree.
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = quote! {
        impl #impl_generics Ffi for #name #ty_generics #where_clause {
            fn call(
                &self,
                ctx: &mut FfiCtx,
                args: Vec<GosValue>,
            ) -> Pin<Box<dyn Future<Output = goscript_vm::value::RuntimeResult<Vec<GosValue>>> + '_>> {
                self.dispatch(ctx, args)
            }
        }
    };

    proc_macro::TokenStream::from(expanded)
}
