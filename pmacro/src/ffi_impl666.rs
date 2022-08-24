// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::LitInt;
use syn::NestedMeta;
use syn::Token;
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
    OneVal,
    MultipleVal,
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
            ctx: &mut FfiCallCtx,
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
                    "FfiCallCtx" => {
                        if i == 0 {
                            args.push_value(parse_quote! {ctx});
                            args.push_punct(Token![,](Span::call_site()));
                        } else {
                            panic!("'FfiCallCtx' should be the first argument")
                        }
                    }
                    "GosValue" => {
                        args.push_value(parse_quote! {arg_iter.next().unwrap()});
                        args.push_punct(Token![,](Span::call_site()));
                        param_count += 1;
                    }
                    _ => panic!("Unexpected FFI argument type"),
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
        (false, true, FfiReturnType::OneVal) => {
            parse_quote! {{
                let re = #self_ty::#callee(#args).map(|x| vec![x]);
                Box::pin(async move { re })
            }}
        }
        (false, true, FfiReturnType::MultipleVal) => {
            parse_quote! {{
                let re: goscript_vm::value::RuntimeResult<Vec<GosValue>>
                    = #self_ty::#callee(#args).map(|x| x.try_into().unwrap());
                Box::pin(async move { re })
            }}
        }
        (false, false, FfiReturnType::ZeroVal) => {
            parse_quote! {{
                #self_ty::#callee(#args);
                Box::pin(async move { Ok(vec![]) })
            }}
        }
        (false, false, FfiReturnType::OneVal) => {
            parse_quote! {{
                let re = #self_ty::#callee(#args);
                Box::pin(async move { Ok(vec![re]) })
            }}
        }
        (false, false, FfiReturnType::MultipleVal) => {
            parse_quote! {{
                let re: Vec<GosValue> = #self_ty::#callee(#args).try_into().unwrap();
                Box::pin(async move { Ok( re ) })
            }}
        }
        (true, true, FfiReturnType::MultipleVal) => {
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
                match seg.ident.to_string().as_str() {
                    "GosValue" => (false, FfiReturnType::OneVal), // todo: futher validation
                    "Pin" => (false, FfiReturnType::AlreadyBoxed), // todo: futher validation
                    "RuntimeResult" => {
                        let inner_type = get_type_arg_type(&seg.arguments);
                        match &inner_type {
                            Type::Path(itp) => {
                                let name = itp.path.segments.last().unwrap().ident.to_string();
                                if name == "GosValue" {
                                    (true, FfiReturnType::OneVal)
                                } else {
                                    return_type_panic!()
                                }
                            }
                            Type::Array(ta) => {
                                let name = get_last_segment(&ta.elem).unwrap().ident.to_string();
                                if name == "GosValue" {
                                    (true, FfiReturnType::MultipleVal)
                                } else {
                                    return_type_panic!()
                                }
                            }
                            Type::Tuple(tt) => {
                                if tt.elems.is_empty() {
                                    (true, FfiReturnType::ZeroVal)
                                } else {
                                    return_type_panic!()
                                }
                            }
                            _ => return_type_panic!(),
                        }
                    }
                    _ => return_type_panic!(),
                }
            }
            Type::Array(_) => (false, FfiReturnType::MultipleVal), // todo: futher validation
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
