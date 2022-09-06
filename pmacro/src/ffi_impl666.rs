// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use proc_macro2::Span;
use proc_macro2::{Literal, TokenStream};
use quote::{quote, ToTokens};
use syn::LitInt;
use syn::NestedMeta;
use syn::Token;
use syn::TypeTuple;
use syn::{
    parse_macro_input, parse_quote, punctuated::Punctuated, Arm, AttributeArgs, Block, Expr, FnArg,
    GenericArgument, Ident, ImplItem, ImplItemMethod, ItemImpl, Lit, Meta, PatType, PathArguments,
    PathSegment, ReturnType, Stmt, Type,
};

const TYPE_ERR_MSG: &str = "unexpected return type";
const FFI_FUNC_PREFIX: &str = "ffi_";

macro_rules! return_type_panic {
    () => {
        panic!("{}", TYPE_ERR_MSG)
    };
}

#[derive(PartialEq)]
enum FfiReturnType {
    ZeroVal,
    OneVal(bool),
    MultipleVal(Vec<bool>),
    Vec,
    AlreadyBoxed,
}

pub fn ffi_impl_implement(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let args: Vec<NestedMeta> = parse_macro_input!(args as AttributeArgs);
    let input: TokenStream = input.into();
    let impl_block =
        syn::parse2::<ItemImpl>(input.clone()).expect("ffi_impl only applies to impl blocks");
    let mut output_block = impl_block.clone();
    let mut output = TokenStream::new();
    let mut new_method: Option<ImplItemMethod> = None;
    let ffi_methods: Vec<&ImplItemMethod> = impl_block
        .items
        .iter()
        .filter_map(|x| match x {
            ImplItem::Method(method) => {
                let ffi_name = method.sig.ident.to_string();
                if ffi_name == "new" {
                    new_method = Some(method.clone());
                    None
                } else {
                    ffi_name.strip_prefix(FFI_FUNC_PREFIX).map(|_| method)
                }
            }
            _ => None,
        })
        .collect();
    let type_name = get_last_segment(&impl_block.self_ty).unwrap().ident;

    let mut methods: Vec<ImplItem> = vec![
        gen_dispatch_method(&impl_block.self_ty, ffi_methods),
        gen_new_method(&type_name, &new_method),
        gen_id_method(&type_name, &args),
        gen_register_method(),
    ]
    .into_iter()
    .map(|x| (ImplItem::Method(x)))
    .collect();

    output_block.items.append(&mut methods);
    output_block.to_tokens(&mut output);
    output.into()
}

fn gen_dispatch_method(self_ty: &Type, ffis: Vec<&ImplItemMethod>) -> ImplItemMethod {
    let mut dispatch_method: ImplItemMethod = parse_quote! {
        fn dispatch(
            &self,
            ctx: &mut FfiCtx,
            args: Vec<GosValue>,
        ) -> Pin<Box<dyn Future<Output = goscript_vm::value::RuntimeResult<Vec<GosValue>>> + '_>> {
            let arg_count = args.len();
            let mut arg_iter = args.into_iter();
            match ctx.func_name {
                _ => unreachable!(),
            }
        }
    };
    let exp = match dispatch_method.block.stmts.last_mut().unwrap() {
        Stmt::Expr(e) => e,
        _ => unreachable!(),
    };
    let match_exp = match exp {
        Expr::Match(me) => me,
        _ => unreachable!(),
    };

    let mut arms: Vec<Arm> = ffis
        .iter()
        .map(|method| {
            let sig = &method.sig;
            if sig.receiver().is_some() {
                panic!("FFI must be an associated function not method, i.e. without 'self'");
            }
            let mut args: Punctuated<Expr, Token![,]> = Punctuated::new();
            let mut param_count = 0;
            for (i, farg) in method.sig.inputs.iter().enumerate() {
                let arg_name: &str = &get_last_segment(&fn_arg_as_pat_type(farg).ty)
                    .unwrap()
                    .ident
                    .to_string();
                match arg_name {
                    "FfiCtx" => {
                        if i == 0 {
                            args.push_value(parse_quote! {ctx});
                            args.push_punct(Token![,](Span::call_site()));
                        } else {
                            panic!("'FfiCtx' should be the first argument")
                        }
                    }
                    "GosValue" => {
                        args.push_value(parse_quote! {arg_iter.next().unwrap()});
                        args.push_punct(Token![,](Span::call_site()));
                        param_count += 1;
                    }
                    _ if is_primitive(arg_name) => {
                        args.push_value(parse_quote! {arg_iter.next().unwrap().as_()});
                        args.push_punct(Token![,](Span::call_site()));
                        param_count += 1;
                    }
                    _ => panic!("Unexpected FFI argument type, only primitive types and GosValue are supported"),
                }
            }
            let count_lit = LitInt::new(&param_count.to_string(), Span::call_site());

            let name = method.sig.ident.to_string();
            let short_name = name.strip_prefix(FFI_FUNC_PREFIX).unwrap();
            let wrapper = gen_wrapper_block(self_ty, &method, &args);
            parse_quote! {
                #short_name => {
                    if arg_count != #count_lit {
                        Box::pin(async move { Err("FFI: bad argument count".to_owned()) })
                    } else {
                        #wrapper
                    }
                },
            }
        })
        .collect();
    arms.push(match_exp.arms.pop().unwrap());
    match_exp.arms = arms;

    dispatch_method
}

fn gen_new_method(type_name: &Ident, new_method: &Option<ImplItemMethod>) -> ImplItemMethod {
    let new_ident = Ident::new("auto_gen_ffi_new", Span::call_site());

    if new_method.is_some() {
        parse_quote! {
            pub fn #new_ident() -> Rc<dyn Ffi> {
                Rc::new(#type_name::new())
            }
        }
    } else {
        parse_quote! {
            pub fn #new_ident() -> Rc<dyn Ffi> {
                Rc::new(#type_name {})
            }
        }
    }
}

fn gen_register_method() -> ImplItemMethod {
    parse_quote! {
        pub fn register(engine: &mut goscript_engine::Engine) {
            engine.register_extension(Self::auto_gen_ffi_id(), Self::auto_gen_ffi_new());
        }
    }
}
fn gen_id_method(type_name: &Ident, meta: &Vec<NestedMeta>) -> ImplItemMethod {
    let rname = meta
        .iter()
        .find_map(|x| match x {
            NestedMeta::Meta(m) => match m {
                Meta::NameValue(nv) => nv
                    .path
                    .segments
                    .last()
                    .map(|seg| match seg.ident.to_string().as_str() {
                        "rename" => match &nv.lit {
                            Lit::Str(s) => Some(s.value()),
                            _ => None,
                        },
                        _ => None,
                    })
                    .flatten(),
                _ => None,
            },
            NestedMeta::Lit(_) => None,
        })
        .unwrap_or({
            let name = type_name.to_string().to_lowercase();
            name.strip_suffix("ffi").unwrap_or(&name).to_string()
        });
    parse_quote! {
        pub fn auto_gen_ffi_id() -> &'static str {
            #rname
        }
    }
}

fn gen_wrapper_block(
    self_ty: &Type,
    m: &ImplItemMethod,
    args: &Punctuated<Expr, Token![,]>,
) -> Block {
    let callee = &m.sig.ident;
    let is_async = m.sig.asyncness.is_some();
    let (is_result, r_type) = get_return_type_attributes(&m.sig.output);
    match (is_async, is_result, r_type) {
        (false, true, FfiReturnType::ZeroVal) => {
            parse_quote! {{
                let re = #self_ty::#callee(#args).map(|x| vec![]);
                Box::pin(async move { re })
            }}
        }
        (false, true, FfiReturnType::OneVal(primitive)) => {
            let ret = one_return_value(primitive);
            parse_quote! {{
                let re = #self_ty::#callee(#args).map(|input| vec![#ret]);
                Box::pin(async move { re })
            }}
        }
        (false, true, FfiReturnType::MultipleVal(types)) => {
            let ret = multiple_return_values(types);
            parse_quote! {{
                let re: goscript_vm::value::RuntimeResult<Vec<GosValue>>
                    = #self_ty::#callee(#args).map(|input| vec![#ret] );
                Box::pin(async move { re })
            }}
        }
        (false, false, FfiReturnType::ZeroVal) => {
            parse_quote! {{
                #self_ty::#callee(#args);
                Box::pin(async move { Ok(vec![]) })
            }}
        }
        (false, false, FfiReturnType::OneVal(primitive)) => {
            let ret = one_return_value(primitive);
            parse_quote! {{
                let input = #self_ty::#callee(#args);
                Box::pin(async move { Ok(vec![#ret]) })
            }}
        }
        (false, false, FfiReturnType::MultipleVal(types)) => {
            let ret = multiple_return_values(types);
            parse_quote! {{
                let input = #self_ty::#callee(#args);
                Box::pin(async move { Ok(vec![#ret]) })
            }}
        }
        (false, _, FfiReturnType::Vec) => panic!("non-async func cannot return a vec"),
        (true, true, FfiReturnType::Vec) => {
            parse_quote! {{
                let re = #self_ty::#callee(#args);
                Box::pin( re )
            }}
        }
        (_, _, FfiReturnType::AlreadyBoxed) => {
            parse_quote! {{
                #self_ty::#callee(#args)
            }}
        }
        (true, _, _) => panic!("async func can only return RuntimeResult<Vec<GosValue>>>"),
    }
}

fn get_return_type_attributes(rt: &ReturnType) -> (bool, FfiReturnType) {
    match rt {
        ReturnType::Default => (false, FfiReturnType::ZeroVal),
        ReturnType::Type(_, t) => match &**t {
            Type::Path(tp) => {
                let seg = tp.path.segments.last().unwrap();
                let type_name = seg.ident.to_string();
                match type_name.as_str() {
                    "GosValue" => (false, FfiReturnType::OneVal(false)), // todo: futher validation
                    _ if is_primitive(&type_name) => (false, FfiReturnType::OneVal(true)),
                    "Pin" => (false, FfiReturnType::AlreadyBoxed), // todo: futher validation
                    "RuntimeResult" => {
                        let inner_type = get_type_arg_type(&seg.arguments);
                        match &inner_type {
                            Type::Path(itp) => {
                                let name = itp.path.segments.last().unwrap().ident.to_string();
                                match name.as_str() {
                                    "GosValue" => (true, FfiReturnType::OneVal(false)),
                                    _ if is_primitive(&name) => (true, FfiReturnType::OneVal(true)),
                                    "Vec" => (true, FfiReturnType::Vec), // todo: futher validation
                                    _ => return_type_panic!(),
                                }
                            }
                            Type::Tuple(tt) => {
                                if tt.elems.is_empty() {
                                    (true, FfiReturnType::ZeroVal)
                                } else {
                                    (true, FfiReturnType::MultipleVal(are_primitives(tt)))
                                }
                            }
                            _ => return_type_panic!(),
                        }
                    }
                    _ => return_type_panic!(),
                }
            }
            Type::Tuple(tt) => {
                if tt.elems.is_empty() {
                    (false, FfiReturnType::ZeroVal)
                } else {
                    (false, FfiReturnType::MultipleVal(are_primitives(tt)))
                }
            }
            _ => return_type_panic!(),
        },
    }
}

fn fn_arg_as_pat_type(arg: &FnArg) -> &PatType {
    match arg {
        FnArg::Typed(pt) => pt,
        _ => unreachable!(),
    }
}

fn get_last_segment(t: &Type) -> Option<PathSegment> {
    match t {
        Type::Path(tp) => Some(tp),
        Type::Reference(tr) => {
            let t: &Type = &*tr.elem;
            match t {
                Type::Path(tp) => Some(tp),
                _ => None,
            }
        }
        _ => None,
    }
    .map(|x| x.path.segments.last().unwrap().clone())
}

fn get_type_arg_type(args: &PathArguments) -> Type {
    match args {
        PathArguments::AngleBracketed(aargs) => {
            let gargs = aargs.args.last().expect(TYPE_ERR_MSG);
            match gargs {
                GenericArgument::Type(t) => t.clone(),
                _ => return_type_panic!(),
            }
        }
        _ => return_type_panic!(),
    }
}

fn is_primitive(name: &str) -> bool {
    match name {
        "bool" | "isize" | "i8" | "i16" | "i32" | "i64" | "usize" | "u8" | "u16" | "u32"
        | "u64" => true,
        _ => false,
    }
}

fn are_primitives(tuple: &TypeTuple) -> Vec<bool> {
    tuple
        .elems
        .iter()
        .map(|x| {
            let name = get_last_segment(x).unwrap().ident.to_string();
            let p = is_primitive(&name);
            if !p && name != "GosValue" {
                return_type_panic!()
            }
            p
        })
        .collect()
}

fn one_return_value(is_primitive: bool) -> TokenStream {
    if is_primitive {
        quote!(input)
    } else {
        quote!(input.into())
    }
}

fn multiple_return_values(types: Vec<bool>) -> Punctuated<Expr, Token![,]> {
    let mut values: Punctuated<Expr, Token![,]> = Punctuated::new();
    for (i, t) in types.into_iter().enumerate() {
        let i_lit = Literal::usize_unsuffixed(i);
        if t {
            values.push_value(parse_quote! {input.#i_lit.into()});
        } else {
            values.push_value(parse_quote! {input.#i_lit});
        }
        values.push_punct(Token![,](Span::call_site()));
    }
    values
}
