#![allow(dead_code)]

use super::constant;
use super::obj::*;
use super::objects::{ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey, Types};
use super::package::*;
use super::scope::*;
use super::typ::*;
use std::collections::HashMap;

/// ExprKind describes the kind of an expression; the kind
/// determines if an expression is valid in 'statement context'.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Conversion,
    Expression,
    Statement,
}

/// A Builtin is the id of a builtin function.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Builtin {
    Append,
    Cap,
    Close,
    Complex,
    Copy,
    Delete,
    Imag,
    Len,
    Make,
    New,
    Panic,
    Print,
    Println,
    Real,
    Recover,
    // package unsafe
    Alignof,
    Offsetof,
    Sizeof,
    // testing support
    Assert,
    Trace,
    // goscript native extension
    Ffi,
}

#[derive(Copy, Clone)]
pub struct BuiltinInfo {
    pub name: &'static str,
    pub arg_count: usize,
    pub variadic: bool,
    pub kind: ExprKind,
}

/// Universe sets up the universe scope, the unsafe package
/// and all the builtin types and functions
pub struct Universe {
    scope: ScopeKey,
    unsafe_: PackageKey,
    iota: ObjKey,
    byte: TypeKey,
    rune: TypeKey,
    slice_of_bytes: TypeKey, // for builtin append
    no_value_tuple: TypeKey, // for OperandMode::NoValue
    // indir is a sentinel type name that is pushed onto the object path
    // to indicate an "indirection" in the dependency from one type name
    // to the next. For instance, for "type p *p" the object path contains
    // p followed by indir, indicating that there's an indirection *p.
    // Indirections are used to break type cycles.
    indir: ObjKey,
    // guard_sig is a empty signature type used to guard against func cycles
    guard_sig: TypeKey,
    types: HashMap<BasicType, TypeKey>,
    builtins: HashMap<Builtin, BuiltinInfo>,
}

impl Universe {
    pub fn new(objs: &mut TCObjects) -> Universe {
        // universe scope and unsafe package
        let (uskey, unsafe_) = Universe::def_universe_unsafe(objs);
        // types
        let types = Universe::basic_types(&mut objs.types);
        Universe::def_basic_types(&types, &uskey, &unsafe_, objs);
        Universe::def_basic_types(
            &Universe::alias_types(&mut objs.types),
            &uskey,
            &unsafe_,
            objs,
        );
        Universe::def_error_type(&types, &uskey, &unsafe_, objs);
        // consts
        Universe::def_consts(&types, &uskey, &unsafe_, objs);
        Universe::def_nil(&types, &uskey, &unsafe_, objs);
        // builtins
        let builtins = Universe::builtin_funcs();
        let ftype = types[&BasicType::Invalid];
        Universe::def_builtins(&builtins, ftype, &uskey, &unsafe_, objs);
        // iota byte rune
        let (iota, byte, rune) = Universe::iota_byte_rune(&uskey, objs);
        let slice_of_bytes = objs.new_t_slice(byte);
        let no_value_tuple = objs.new_t_tuple(vec![]);
        let indir = objs.new_type_name(0, None, "*".to_string(), None);
        let guard_sig = objs.new_t_signature(None, None, no_value_tuple, no_value_tuple, false);
        Universe {
            scope: uskey,
            unsafe_: unsafe_,
            iota: iota,
            byte: byte,
            rune: rune,
            slice_of_bytes: slice_of_bytes,
            no_value_tuple: no_value_tuple,
            indir: indir,
            guard_sig: guard_sig,
            types: types,
            builtins: builtins,
        }
    }

    pub fn scope(&self) -> &ScopeKey {
        &self.scope
    }

    pub fn unsafe_pkg(&self) -> &PackageKey {
        &self.unsafe_
    }

    pub fn types(&self) -> &HashMap<BasicType, TypeKey> {
        &self.types
    }

    pub fn builtins(&self) -> &HashMap<Builtin, BuiltinInfo> {
        &self.builtins
    }

    pub fn iota(&self) -> &ObjKey {
        &self.iota
    }

    pub fn byte(&self) -> &TypeKey {
        &self.byte
    }

    pub fn rune(&self) -> &TypeKey {
        &self.rune
    }

    pub fn slice_of_bytes(&self) -> &TypeKey {
        &self.slice_of_bytes
    }

    pub fn no_value_tuple(&self) -> &TypeKey {
        &self.no_value_tuple
    }

    pub fn indir(&self) -> &ObjKey {
        &self.indir
    }

    pub fn guard_sig(&self) -> &TypeKey {
        &self.guard_sig
    }

    fn def_universe_unsafe(objs: &mut TCObjects) -> (ScopeKey, PackageKey) {
        let uskey = objs
            .scopes
            .insert(Scope::new(None, 0, 0, "universe".to_owned(), false));
        let pkg_skey = objs.scopes.insert(Scope::new(
            Some(uskey),
            0,
            0,
            "package unsafe".to_owned(),
            false,
        ));
        let mut pkg = Package::new("unsafe".to_owned(), Some("unsafe".to_owned()), pkg_skey);
        pkg.mark_complete();
        (uskey, objs.pkgs.insert(pkg))
    }

    ///define this:
    ///type error interface {
    ///    Error() string
    ///}
    fn def_error_type(
        types: &HashMap<BasicType, TypeKey>,
        universe: &ScopeKey,
        unsafe_: &PackageKey,
        objs: &mut TCObjects,
    ) {
        let res = objs.lobjs.insert(LangObj::new_var(
            0,
            None,
            "".to_owned(),
            Some(types[&BasicType::Str]),
        ));
        let params = objs.new_t_tuple(vec![]);
        let results = objs.new_t_tuple(vec![res]);
        let sig = objs.new_t_signature(None, None, params, results, false);
        let err = objs
            .lobjs
            .insert(LangObj::new_func(0, None, "Error".to_owned(), Some(sig)));
        let inter_detail = InterfaceDetail::new(vec![err], vec![], objs);
        inter_detail.complete(objs);
        let underlying = objs.types.insert(Type::Interface(inter_detail));
        let typ = objs.new_t_named(None, Some(underlying), vec![]);
        let recv = objs
            .lobjs
            .insert(LangObj::new_var(0, None, "".to_owned(), Some(typ)));
        objs.types[sig]
            .try_as_signature_mut()
            .unwrap()
            .set_recv(Some(recv));
        let type_name = objs.lobjs.insert(LangObj::new_type_name(
            0,
            None,
            "error".to_owned(),
            Some(typ),
        ));
        Universe::def(type_name, universe, unsafe_, objs);
    }

    fn def_basic_types(
        types: &HashMap<BasicType, TypeKey>,
        universe: &ScopeKey,
        unsafe_: &PackageKey,
        objs: &mut TCObjects,
    ) {
        for (_, v) in types {
            let name = objs.types[*v].try_as_basic().unwrap().name();
            let t = objs
                .lobjs
                .insert(LangObj::new_type_name(0, None, name.to_owned(), Some(*v)));
            Universe::def(t, universe, unsafe_, objs);
        }
    }

    fn basic_types(tobjs: &mut Types) -> HashMap<BasicType, TypeKey> {
        vec![
            // use vec becasue array doesn't have into_iter()!
            // may be more expensive, but looks better this way.
            (BasicType::Invalid, BasicInfo::IsInvalid, "invalid type"),
            (BasicType::Bool, BasicInfo::IsBoolean, "bool"),
            (BasicType::Int, BasicInfo::IsInteger, "int"),
            (BasicType::Int8, BasicInfo::IsInteger, "int8"),
            (BasicType::Int16, BasicInfo::IsInteger, "int16"),
            (BasicType::Int32, BasicInfo::IsInteger, "int32"),
            (BasicType::Int64, BasicInfo::IsInteger, "int64"),
            (BasicType::Uint, BasicInfo::IsInteger, "uint"),
            (BasicType::Uint8, BasicInfo::IsInteger, "uint8"),
            (BasicType::Uint16, BasicInfo::IsInteger, "uint16"),
            (BasicType::Uint32, BasicInfo::IsInteger, "uint32"),
            (BasicType::Uint64, BasicInfo::IsInteger, "uint64"),
            (BasicType::Uintptr, BasicInfo::IsInteger, "uintptr"),
            (BasicType::Float32, BasicInfo::IsFloat, "float32"),
            (BasicType::Float64, BasicInfo::IsFloat, "float64"),
            (BasicType::Complex64, BasicInfo::IsComplex, "complex64"),
            (BasicType::Complex128, BasicInfo::IsComplex, "complex128"),
            (BasicType::Str, BasicInfo::IsString, "string"),
            (BasicType::UnsafePointer, BasicInfo::IsInvalid, "Pointer"),
            (BasicType::UntypedBool, BasicInfo::IsBoolean, "untyped bool"),
            (BasicType::UntypedInt, BasicInfo::IsInteger, "untyped int"),
            (BasicType::UntypedRune, BasicInfo::IsInteger, "untyped rune"),
            (BasicType::UntypedFloat, BasicInfo::IsFloat, "untyped float"),
            (
                BasicType::UntypedComplex,
                BasicInfo::IsComplex,
                "untyped complex",
            ),
            (
                BasicType::UntypedString,
                BasicInfo::IsString,
                "untyped string",
            ),
            (BasicType::UntypedNil, BasicInfo::IsInvalid, "untyped nil"),
        ]
        .into_iter()
        .map(|(t, i, n)| (t, tobjs.insert(Type::Basic(BasicDetail::new(t, i, n)))))
        .collect::<HashMap<_, _>>()
    }

    fn alias_types(tobjs: &mut Types) -> HashMap<BasicType, TypeKey> {
        [
            (BasicType::Byte, BasicInfo::IsInteger, "byte"),
            (BasicType::Rune, BasicInfo::IsInteger, "rune"),
        ]
        .iter()
        .map(|(t, i, n)| (*t, tobjs.insert(Type::Basic(BasicDetail::new(*t, *i, n)))))
        .collect::<HashMap<_, _>>()
    }

    fn def_consts(
        types: &HashMap<BasicType, TypeKey>,
        universe: &ScopeKey,
        unsafe_: &PackageKey,
        objs: &mut TCObjects,
    ) {
        for (n, t, v) in vec![
            // use vec becasue array doesn't have into_iter()!
            (
                "true",
                BasicType::UntypedBool,
                constant::Value::with_bool(true),
            ),
            (
                "false",
                BasicType::UntypedBool,
                constant::Value::with_bool(false),
            ),
            ("iota", BasicType::UntypedInt, constant::Value::with_i64(0)),
        ]
        .into_iter()
        {
            let cst = LangObj::new_const(0, None, n.to_owned(), Some(types[&t]), v);
            Universe::def(objs.lobjs.insert(cst), universe, unsafe_, objs);
        }
    }

    fn def_nil(
        types: &HashMap<BasicType, TypeKey>,
        universe: &ScopeKey,
        unsafe_: &PackageKey,
        objs: &mut TCObjects,
    ) {
        let nil = LangObj::new_nil(types[&BasicType::UntypedNil]);
        Universe::def(objs.lobjs.insert(nil), universe, unsafe_, objs);
    }

    fn builtin_funcs() -> HashMap<Builtin, BuiltinInfo> {
        vec![
            // use vec becasue array doesn't have into_iter()!
            (Builtin::Append, "append", 1, true, ExprKind::Expression),
            (Builtin::Cap, "cap", 1, false, ExprKind::Expression),
            (Builtin::Close, "close", 1, false, ExprKind::Statement),
            (Builtin::Complex, "complex", 2, false, ExprKind::Expression),
            (Builtin::Copy, "copy", 2, false, ExprKind::Statement),
            (Builtin::Delete, "delete", 2, false, ExprKind::Statement),
            (Builtin::Imag, "imag", 1, false, ExprKind::Expression),
            (Builtin::Len, "len", 1, false, ExprKind::Expression),
            (Builtin::Make, "make", 1, true, ExprKind::Expression),
            (Builtin::New, "new", 1, false, ExprKind::Expression),
            (Builtin::Panic, "panic", 1, false, ExprKind::Statement),
            (Builtin::Print, "print", 0, true, ExprKind::Statement),
            (Builtin::Println, "println", 0, true, ExprKind::Statement),
            (Builtin::Real, "real", 1, false, ExprKind::Expression),
            (Builtin::Recover, "recover", 0, false, ExprKind::Statement),
            (Builtin::Alignof, "Alignof", 1, false, ExprKind::Expression),
            (
                Builtin::Offsetof,
                "Offsetof",
                1,
                false,
                ExprKind::Expression,
            ),
            (Builtin::Sizeof, "Sizeof", 1, false, ExprKind::Expression),
            (Builtin::Assert, "assert", 1, false, ExprKind::Statement),
            (Builtin::Trace, "trace", 0, true, ExprKind::Statement),
            (Builtin::Ffi, "ffi", 2, false, ExprKind::Expression),
        ]
        .into_iter()
        .map(|(f, name, no, v, k)| {
            (
                f,
                BuiltinInfo {
                    name: name,
                    arg_count: no,
                    variadic: v,
                    kind: k,
                },
            )
        })
        .collect::<HashMap<_, _>>()
    }

    fn def_builtins(
        builtins: &HashMap<Builtin, BuiltinInfo>,
        typ: TypeKey,
        universe: &ScopeKey,
        unsafe_: &PackageKey,
        objs: &mut TCObjects,
    ) {
        for (f, info) in builtins.iter() {
            let fobj = objs
                .lobjs
                .insert(LangObj::new_builtin(*f, info.name.to_owned(), typ));
            Universe::def(fobj, universe, unsafe_, objs);
        }
    }

    fn iota_byte_rune(universe: &ScopeKey, objs: &mut TCObjects) -> (ObjKey, TypeKey, TypeKey) {
        let scope = &objs.scopes[*universe];
        let iota = *scope.lookup("iota").unwrap();
        let byte = objs.lobjs[*scope.lookup("byte").unwrap()].typ().unwrap();
        let rune = objs.lobjs[*scope.lookup("rune").unwrap()].typ().unwrap();
        (iota, byte, rune)
    }

    // Objects with names containing blanks are internal and not entered into
    // a scope. Objects with exported names are inserted in the unsafe package
    // scope; other objects are inserted in the universe scope.
    fn def(okey: ObjKey, universe: &ScopeKey, unsafe_: &PackageKey, objs: &mut TCObjects) {
        let obj = &objs.lobjs[okey];
        assert!(obj.color() == ObjColor::Black);
        if obj.name().contains(' ') {
            return;
        }
        // fix Obj link for named types
        let typ_val = &mut objs.types[obj.typ().unwrap()];
        if let Some(n) = typ_val.try_as_named_mut() {
            n.set_obj(okey);
        }

        let skey = if obj.exported() {
            match obj.entity_type() {
                EntityType::TypeName | EntityType::Builtin(_) => {
                    objs.lobjs[okey].set_pkg(Some(*unsafe_))
                }
                _ => unreachable!(),
            }
            objs.pkgs[*unsafe_].scope()
        } else {
            universe
        };
        let re = Scope::insert(*skey, okey, objs);
        assert!(re.is_none());
    }
}
