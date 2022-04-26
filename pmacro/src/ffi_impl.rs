// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::NestedMeta;
use syn::Token;
use syn::{
    parse_macro_input, parse_quote, punctuated::Punctuated, Arm, AttributeArgs, Expr, FnArg,
    GenericArgument, Ident, ImplItem, ImplItemMethod, ItemImpl, Lit, Meta, Pat, PatType,
    PathArguments, PathSegment, ReturnType, Signature, Stmt, Type,
};

const TYPE_ERR_MSG: &str = "unexpected return type";
const FFI_FUNC_PREFIX: &str = "ffi_";
const WRAPPER_FUNC_PREFIX: &str = "wrapper_ffi_";

macro_rules! type_panic {
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
    let func_name_args: Vec<(String, Vec<Box<Type>>)> = impl_block
        .items
        .iter()
        .filter_map(|x| match x {
            ImplItem::Method(method) => {
                let ffi_name = method.sig.ident.to_string();
                if ffi_name == "new" {
                    new_method = Some(method.clone());
                    None
                } else {
                    ffi_name.strip_prefix(FFI_FUNC_PREFIX).map(|x| {
                        let wrapper_name = format!("{}{}", WRAPPER_FUNC_PREFIX, x);
                        let m = gen_wrapper_method(&method, &wrapper_name);
                        output_block.items.push(ImplItem::Method(m));
                        (wrapper_name, get_arg_types(&method.sig))
                    })
                }
            }
            _ => None,
        })
        .collect();
    let type_name = get_type_name(&impl_block.self_ty).unwrap().ident;

    if new_method.is_none() {
        panic!("new method not found!");
    }

    let mut methods: Vec<ImplItem> = vec![
        gen_dispatch_method(&func_name_args),
        get_wrapper_new_method(&type_name, &new_method.unwrap()),
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

fn gen_dispatch_method(name_args: &Vec<(String, Vec<Box<Type>>)>) -> ImplItemMethod {
    let mut method: ImplItemMethod = parse_quote! {
        fn dispatch(
            &self,
            ctx: &mut FfiCallCtx,
            args: Vec<GosValue>,
        ) -> Pin<Box<dyn Future<Output = goscript_vm::value::RuntimeResult<Vec<GosValue>>> + '_>> {
            match ctx.func_name {
                "dummy" => self.dummy(ctx, args),
                _ => unreachable!(),
            }
        }
    };
    let exp = match method.block.stmts.last_mut().unwrap() {
        Stmt::Expr(e) => e,
        _ => unreachable!(),
    };
    let match_exp = match exp {
        Expr::Match(me) => me,
        _ => unreachable!(),
    };
    let mut arms: Vec<Arm> = name_args
        .iter()
        .map(|x| {
            let name = &x.0;
            let short_name = name.strip_prefix(WRAPPER_FUNC_PREFIX).unwrap();
            let ident = Ident::new(&name, Span::call_site());
            let arg_types = &x.1;
            let mut args: Punctuated<Box<Pat>, Token![,]> = Punctuated::new();
            args = arg_types.iter().fold(args, |mut acc, x| {
                let x_type_name = get_type_name(x).unwrap().ident;
                let arg = method.sig.inputs.iter().skip(1).find(|fnarg| {
                    get_type_name(&fn_arg_as_pat_type(fnarg).ty).unwrap().ident == x_type_name
                });
                acc.push_value(fn_arg_as_pat_type(arg.unwrap()).pat.clone());
                acc.push_punct(Token![,](Span::call_site()));
                acc
            });
            parse_quote! {
                #short_name => self.#ident(#args),
            }
        })
        .collect();
    arms.push(match_exp.arms.pop().unwrap());
    match_exp.arms = arms;

    method
}

fn get_wrapper_new_method(type_name: &Ident, method: &ImplItemMethod) -> ImplItemMethod {
    let wrapper_ident = Ident::new("wrapper_new", Span::call_site());
    let (is_result, rcount) =
        get_return_type_attributes(&method.sig.output, &type_name.to_string());
    if rcount != FfiReturnType::OneVal {
        panic!("the new method must have one and only one return value");
    }
    if is_result {
        panic!("the new method must not fail");
    }
    parse_quote! {
        pub fn #wrapper_ident(args: Vec<GosValue>) -> FfiCtorResult<Rc<RefCell<dyn Ffi>>> {
            Ok(Rc::new(RefCell::new(#type_name::new(args))))
        }
    }
}

fn gen_register_method() -> ImplItemMethod {
    parse_quote! {
        pub fn register(engine: &mut goscript_engine::Engine) {
            engine.register_extension(Self::ffi__id(), Box::new(Self::wrapper_new));
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
        pub fn ffi__id() -> &'static str {
            #rname
        }
    }
}

fn gen_wrapper_method(m: &ImplItemMethod, name: &str) -> ImplItemMethod {
    let mut wrapper = m.clone();
    wrapper.sig.ident = Ident::new(name, Span::call_site());
    let callee = &m.sig.ident;
    let args = get_args(&m.sig);
    let is_async = m.sig.asyncness.is_some();
    let (is_result, rcount) = get_return_type_attributes(&m.sig.output, "GosValue");
    wrapper.block = match (is_async, is_result, rcount) {
        (false, true, FfiReturnType::ZeroVal) => {
            parse_quote! {{
                let re = self.#callee(#args).map(|x| vec![]);
                Box::pin(async move { re })
            }}
        }
        (false, true, FfiReturnType::OneVal) => {
            parse_quote! {{
                let re = self.#callee(#args).map(|x| vec![x]);
                Box::pin(async move { re })
            }}
        }
        (false, true, FfiReturnType::MultipleVal) => {
            parse_quote! {{
                let re = self.#callee(#args);
                Box::pin(async move { re })
            }}
        }
        (false, false, FfiReturnType::ZeroVal) => {
            parse_quote! {{
                self.#callee(#args);
                Box::pin(async move { Ok(vec![]) })
            }}
        }
        (false, false, FfiReturnType::OneVal) => {
            parse_quote! {{
                let re = self.#callee(#args);
                Box::pin(async move { Ok(vec![re]) })
            }}
        }
        (false, false, FfiReturnType::MultipleVal) => {
            parse_quote! {{
                let re = self.#callee(#args);
                Box::pin(async move { Ok( re ) })
            }}
        }
        (true, true, FfiReturnType::MultipleVal) => {
            parse_quote! {{
                let re = self.#callee(#args);
                Box::pin( re )
            }}
        }
        (_, _, FfiReturnType::AlreadyBoxed) => {
            parse_quote! {{
                self.#callee(#args)
            }}
        }
        (true, _, _) => panic!("async func can only return RuntimeResult<Vec<GosValue>>>"),
    };

    wrapper.sig.output = parse_quote! {-> Pin<Box<dyn Future<Output = goscript_vm::value::RuntimeResult<Vec<GosValue>>> + '_>>};
    wrapper.sig.asyncness = None;
    wrapper
}

fn get_return_type_attributes(rt: &ReturnType, return_elem_name: &str) -> (bool, FfiReturnType) {
    match rt {
        ReturnType::Default => (false, FfiReturnType::ZeroVal),
        ReturnType::Type(_, t) => match get_type_name(t) {
            Some(seg) => {
                let typ_name: &str = &seg.ident.to_string();
                match typ_name {
                    "RuntimeResult" => match get_type_name(&get_type_arg_type(&seg.arguments)) {
                        Some(seg) => {
                            let typ_name: &str = &seg.ident.to_string();
                            match typ_name {
                                "Vec" => (true, FfiReturnType::MultipleVal), // todo: futher validation
                                _ if typ_name == return_elem_name => (true, FfiReturnType::OneVal),
                                _ => type_panic!(),
                            }
                        }
                        None => (true, FfiReturnType::ZeroVal), // todo: futher validation
                    },
                    "Vec" => (false, FfiReturnType::MultipleVal), // todo: futher validation
                    _ if typ_name == return_elem_name => (false, FfiReturnType::OneVal), // todo: futher validation
                    "Pin" => (false, FfiReturnType::AlreadyBoxed), // todo: futher validation
                    _ => type_panic!(),
                }
            }
            None => type_panic!(),
        },
    }
}

fn fn_arg_as_pat_type(arg: &FnArg) -> &PatType {
    match arg {
        FnArg::Typed(pt) => pt,
        _ => unreachable!(),
    }
}

fn get_type_name(t: &Type) -> Option<PathSegment> {
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
                _ => type_panic!(),
            }
        }
        _ => type_panic!(),
    }
}

fn get_args(sig: &Signature) -> Punctuated<Box<Pat>, Token![,]> {
    let init: Punctuated<Box<Pat>, Token![,]> = Punctuated::new();
    sig.inputs.iter().fold(init, |mut acc, x| match x {
        FnArg::Typed(pt) => {
            acc.push_value(pt.pat.clone());
            acc.push_punct(Token![,](Span::call_site()));
            acc
        }
        _ => acc,
    })
}

fn get_arg_types(sig: &Signature) -> Vec<Box<Type>> {
    sig.inputs
        .iter()
        .filter_map(|x| match x {
            FnArg::Typed(pt) => Some(pt.ty.clone()),
            _ => None,
        })
        .collect()
}
