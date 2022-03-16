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

enum ReturnedCount {
    Zero,
    One,
    Multiple,
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
    let func_name_args: Vec<(String, Vec<Box<Type>>)> = impl_block
        .items
        .iter()
        .filter_map(|x| match x {
            ImplItem::Method(method) => {
                let ffi_name = method.sig.ident.to_string();
                ffi_name.strip_prefix(FFI_FUNC_PREFIX).map(|x| {
                    let wrapper_name = format!("{}{}", WRAPPER_FUNC_PREFIX, x);
                    let m = gen_wrapper_method(&method, &wrapper_name);
                    output_block.items.push(ImplItem::Method(m));
                    (wrapper_name, get_arg_types(&method.sig))
                })
            }
            _ => None,
        })
        .collect();
    let type_name = get_type_name(&impl_block.self_ty).unwrap().ident;

    let mut methods: Vec<ImplItem> = vec![
        gen_dispatch_method(&func_name_args),
        get_wrapper_new_method(&type_name),
        gen_register_method(&type_name, &args),
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
            ctx: &FfiCallCtx,
            params: Vec<GosValue>,
        ) -> Pin<Box<dyn Future<Output = goscript_vm::value::RuntimeResult<Vec<GosValue>>> + '_>> {
            match ctx.func_name {
                "dummy" => self.dummy(ctx, params),
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

fn get_wrapper_new_method(type_name: &Ident) -> ImplItemMethod {
    let wrapper_ident = Ident::new("wrapper_new", Span::call_site());
    parse_quote! {
        pub fn #wrapper_ident(params: Vec<GosValue>) -> FfiCtorResult<Rc<RefCell<dyn Ffi>>> {
            Ok(Rc::new(RefCell::new(#type_name::new(params))))
        }
    }
}

fn gen_register_method(type_name: &Ident, meta: &Vec<NestedMeta>) -> ImplItemMethod {
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
        .unwrap_or(type_name.to_string().to_lowercase());
    parse_quote! {
        pub fn register(engine: &mut goscript_engine::Engine) {
            engine.register_extension(#rname, Box::new(Self::wrapper_new));
        }
    }
}

fn gen_wrapper_method(m: &ImplItemMethod, name: &str) -> ImplItemMethod {
    let mut wrapper = m.clone();
    wrapper.sig.ident = Ident::new(name, Span::call_site());
    let callee = &m.sig.ident;
    let args = get_args(&m.sig);
    let is_async = m.sig.asyncness.is_some();
    let (is_result, rcount) = get_return_type_attributes(&m.sig.output);
    wrapper.block = match (is_async, is_result, rcount) {
        (false, true, ReturnedCount::Zero) => {
            parse_quote! {{
                let re = self.#callee(#args).map(|x| vec![]);
                Box::pin(async move { re })
            }}
        }
        (false, true, ReturnedCount::One) => {
            parse_quote! {{
                let re = self.#callee(#args).map(|x| vec![x]);
                Box::pin(async move { re })
            }}
        }
        (false, true, ReturnedCount::Multiple) => {
            parse_quote! {{
                let re = self.#callee(#args);
                Box::pin(async move { re })
            }}
        }
        (false, false, ReturnedCount::Zero) => {
            parse_quote! {{
                self.#callee(#args);
                Box::pin(async move { Ok(vec![]) })
            }}
        }
        (false, false, ReturnedCount::One) => {
            parse_quote! {{
                let re = self.#callee(#args);
                Box::pin(async move { Ok(vec![re]) })
            }}
        }
        (false, false, ReturnedCount::Multiple) => {
            parse_quote! {{
                let re = self.#callee(#args);
                Box::pin(async move { Ok( re ) })
            }}
        }
        (true, true, ReturnedCount::Multiple) => {
            parse_quote! {{
                let re = self.#callee(#args);
                Box::pin( re )
            }}
        }
        (true, _, _) => panic!("async func can only return RuntimeResult<Vec<GosValue>>>"),
    };

    wrapper.sig.output = parse_quote! {-> Pin<Box<dyn Future<Output = goscript_vm::value::RuntimeResult<Vec<GosValue>>> + '_>>};
    wrapper.sig.asyncness = None;
    wrapper
}

fn get_return_type_attributes(rt: &ReturnType) -> (bool, ReturnedCount) {
    match rt {
        ReturnType::Default => (false, ReturnedCount::Zero),
        ReturnType::Type(_, t) => match get_type_name(t) {
            Some(seg) => {
                let typ_name: &str = &seg.ident.to_string();
                match typ_name {
                    "RuntimeResult" => match get_type_name(&get_type_arg_type(&seg.arguments)) {
                        Some(seg) => {
                            let typ_name: &str = &seg.ident.to_string();
                            match typ_name {
                                "Vec" => (true, ReturnedCount::Multiple), // todo: futher validation
                                "GosValue" => (true, ReturnedCount::One),
                                _ => type_panic!(),
                            }
                        }
                        None => (true, ReturnedCount::Zero), // todo: futher validation
                    },
                    "Vec" => (false, ReturnedCount::Multiple), // todo: futher validation
                    "GosValue" => (false, ReturnedCount::One),
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
