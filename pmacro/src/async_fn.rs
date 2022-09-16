// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{ItemFn, Token};

pub fn async_fn_implement(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input: TokenStream = input.into();
    let mut func = syn::parse2::<ItemFn>(input.clone()).expect("async only applies to a function");
    func.sig.asyncness = Some(Token![async](Span::call_site()));
    let mut output = TokenStream::new();
    func.to_tokens(&mut output);
    output.into()
}
