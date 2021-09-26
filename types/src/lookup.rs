#![allow(dead_code)]
use super::obj;
use super::objects::{ObjKey, PackageKey, TCObjects, TypeKey};
use super::selection::*;
use super::typ;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Write;

macro_rules! lookup_on_found {
    ($indices:ident, $i:ident, $target:expr, $et:ident, $indirect:ident, $found:expr) => {
        $indices = concat_vec($et.indices.clone(), $i);
        if $target.is_some() || $et.multiples {
            return LookupResult::Ambiguous($indices.unwrap());
        }
        *$target = Some($found);
        $indirect = $et.indirect;
    };
}

/// the result of lookup_field_or_method
#[derive(PartialEq)]
pub enum LookupResult {
    /// valid entry
    Entry(ObjKey, Vec<usize>, bool),
    /// the index sequence points to an ambiguous entry
    /// (the same name appeared more than once at the same embedding level).
    Ambiguous(Vec<usize>),
    /// a method with a pointer receiver type was found
    /// but there was no pointer on the path from the actual receiver type to
    /// the method's formal receiver base type, nor was the receiver addressable.
    BadMethodReceiver,
    /// nothing found
    NotFound,
}

pub struct MethodSet {
    list: Vec<Selection>,
}

impl MethodSet {
    pub fn new(t: &TypeKey, objs: &mut TCObjects) -> MethodSet {
        // method set up to the current depth
        let mut mset_base: HashMap<String, MethodCollision> = HashMap::new();
        let (tkey, is_ptr) = try_deref(*t, objs);
        // *typ where typ is an interface has no methods.
        if is_ptr && objs.types[tkey].try_as_interface().is_some() {
            return MethodSet { list: vec![] };
        }

        // Start with typ as single entry at shallowest depth.
        let mut current = vec![EmbeddedType::new(tkey, None, is_ptr, false)];

        // Named types that we have seen already, allocated lazily.
        // Used to avoid endless searches in case of recursive types.
        // Since only Named types can be used for recursive types, we
        // only need to track those.
        // (If we ever allow type aliases to construct recursive types,
        // we must use type identity rather than pointer equality for
        // the map key comparison, as we do in consolidate_multiples.)
        // seen is lazily initialized
        let mut seen: Option<HashSet<TypeKey>> = None;
        while !current.is_empty() {
            // embedded types found at current depth
            let mut next = vec![];
            // field and method sets at current depth
            let mut fset: HashMap<String, FieldCollision> = HashMap::new();
            let mut mset: HashMap<String, MethodCollision> = HashMap::new();
            for et in current.iter() {
                let mut tobj = &objs.types[et.typ];
                if let typ::Type::Named(detail) = tobj {
                    if seen.is_none() {
                        seen = Some(HashSet::new());
                    }
                    let seen_mut = seen.as_mut().unwrap();
                    if seen_mut.contains(&et.typ) {
                        // We have seen this type before, at a more shallow depth
                        // (note that multiples of this type at the current depth
                        // were consolidated before). The type at that depth shadows
                        // this same type at the current depth, so we can ignore
                        // this one.
                        continue;
                    }
                    seen_mut.insert(et.typ);
                    add_to_method_set(
                        &mut mset,
                        detail.methods(),
                        et.indices.as_ref().unwrap_or(&vec![]),
                        et.indirect,
                        et.multiples,
                        objs,
                    );
                    // continue with underlying type
                    tobj = &objs.types[detail.underlying()];
                }
                match tobj {
                    typ::Type::Struct(detail) => {
                        for (i, f) in detail.fields().iter().enumerate() {
                            let fobj = &objs.lobjs[*f];
                            add_to_field_set(&mut fset, f, et.multiples, objs);
                            // Embedded fields are always of the form T or *T where
                            // T is a type name. If typ appeared multiple times at
                            // this depth, f.Type appears multiple times at the next
                            // depth.
                            if fobj.var_embedded() {
                                let (tkey, is_ptr) = try_deref(fobj.typ().unwrap(), objs);
                                next.push(EmbeddedType::new(
                                    tkey,
                                    concat_vec(et.indices.clone(), i),
                                    et.indirect || is_ptr,
                                    et.multiples,
                                ))
                            }
                        }
                    }
                    typ::Type::Interface(detail) => {
                        add_to_method_set(
                            &mut mset,
                            detail.all_methods().as_ref().unwrap(),
                            et.indices.as_ref().unwrap_or(&vec![]),
                            true,
                            et.multiples,
                            objs,
                        );
                    }
                    _ => {}
                }
            }
            // Add methods and collisions at this depth to base if no entries with matching
            // names exist already.
            for (k, m) in mset.iter() {
                if !mset_base.contains_key(k) {
                    mset_base.insert(
                        k.clone(),
                        if fset.contains_key(k) {
                            MethodCollision::Collision
                        } else {
                            m.clone()
                        },
                    );
                }
            }
            // Multiple fields with matching names collide at this depth and shadow all
            // entries further down; add them as collisions to base if no entries with
            // matching names exist already.
            for (k, f) in fset.iter() {
                if *f == FieldCollision::Collision {
                    if !mset_base.contains_key(k) {
                        mset_base.insert(k.clone(), MethodCollision::Collision);
                    }
                }
            }
            current = consolidate_multiples(next, objs);
        }
        let mut list: Vec<Selection> = mset_base
            .into_iter()
            .filter_map(|(_, m)| match m {
                MethodCollision::Method(sel) => {
                    //sel.init(objs);
                    Some(sel)
                }
                MethodCollision::Collision => None,
            })
            .collect();
        list.sort_by(|a, b| a.id().cmp(b.id()));
        MethodSet { list: list }
    }

    pub fn list(&self) -> &Vec<Selection> {
        &self.list
    }

    pub fn lookup(&self, pkgkey: &PackageKey, name: &str, objs: &TCObjects) -> Option<&Selection> {
        if self.list.len() == 0 {
            return None;
        }
        let pkg = &objs.pkgs[*pkgkey];
        let id = obj::get_id(Some(pkg), name).to_string();
        self.list
            .binary_search_by_key(&&id, |x| x.id())
            .ok()
            .map(|i| &self.list[i])
    }

    pub fn fmt(&self, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
        f.write_str("MethodSet {")?;
        for field in self.list.iter() {
            field.fmt(f, objs)?;
        }
        f.write_char('}')
    }
}

/// lookup_field_or_method looks up a field or method with given package and name
/// in T and returns the corresponding *Var or *Func, an index sequence, and a
/// bool indicating if there were any pointer indirections on the path to the
/// field or method. If addressable is set, T is the type of an addressable
/// variable (only matters for method lookups).
///
/// The last index entry is the field or method index in the (possibly embedded)
/// type where the entry was found, either:
///
///	1) the list of declared methods of a named type; or
///	2) the list of all methods (method set) of an interface type; or
///	3) the list of fields of a struct type.
///
/// The earlier index entries are the indices of the embedded struct fields
/// traversed to get to the found entry, starting at depth 0.
pub fn lookup_field_or_method(
    tkey: TypeKey,
    addressable: bool,
    pkg: Option<PackageKey>,
    name: &str,
    objs: &TCObjects,
) -> LookupResult {
    if let Some(named) = &objs.types[tkey].try_as_named() {
        // Methods cannot be associated to a named pointer type
        // (spec: "The type denoted by T is called the receiver base type;
        // it must not be a pointer or interface type and it must be declared
        // in the same package as the method.").
        // Thus, if we have a named pointer type, proceed with the underlying
        // pointer type but discard the result if it is a method since we would
        // not have found it for T
        let pkey = named.underlying();
        if objs.types[pkey].try_as_pointer().is_some() {
            let re = lookup_field_or_method_impl(pkey, false, pkg, name, objs);
            if let LookupResult::Entry(okey, _, _) = &re {
                if objs.lobjs[*okey].entity_type().is_func() {
                    return LookupResult::NotFound;
                }
            }
            return re;
        }
    }
    lookup_field_or_method_impl(tkey, addressable, pkg, name, objs)
}

/// assertable_to reports whether a value of type iface can be asserted to have type t.
/// It returns None as affirmative answer. See docs for missing_method for more info
pub fn assertable_to(iface: TypeKey, t: TypeKey, objs: &TCObjects) -> Option<(ObjKey, bool)> {
    // no static check is required if T is an interface
    // spec: "If T is an interface type, x.(T) asserts that the
    //        dynamic type of x implements the interface T."
    let strict = true; // see original go code for more info
    if !strict && objs.types[t].is_interface(objs) {
        return None;
    }
    missing_method(t, iface, false, objs)
}

/// try_deref dereferences t if it is a *Pointer and returns its base.
/// Otherwise it returns t.
pub fn try_deref(t: TypeKey, objs: &TCObjects) -> (TypeKey, bool) {
    match &objs.types[t] {
        typ::Type::Pointer(detail) => (detail.base(), true),
        _ => (t, false),
    }
}

/// deref_struct_ptr dereferences typ if it is a (named or unnamed) pointer to a
/// (named or unnamed) struct and returns its base. Otherwise it returns typ.   
pub fn deref_struct_ptr(t: TypeKey, objs: &TCObjects) -> TypeKey {
    let ut = typ::underlying_type(t, objs);
    match &objs.types[ut] {
        typ::Type::Pointer(detail) => {
            let but = typ::underlying_type(detail.base(), objs);
            match &objs.types[but] {
                typ::Type::Struct(_) => but,
                _ => t,
            }
        }
        _ => t,
    }
}

/// field_index returns the index for the field with matching package and name.
pub fn field_index(
    fields: &Vec<ObjKey>,
    pkg: Option<PackageKey>,
    name: &str,
    objs: &TCObjects,
) -> Option<usize> {
    if name != "_" {
        fields
            .iter()
            .enumerate()
            .find(|(_i, x)| objs.lobjs[**x].same_id(pkg, name, objs))
            .map(|(i, _x)| i)
    } else {
        None
    }
}
/// lookup_method returns the index of and method with matching package and name.
pub fn lookup_method<'a>(
    methods: &'a Vec<ObjKey>,
    pkg: Option<PackageKey>,
    name: &str,
    objs: &TCObjects,
) -> Option<(usize, &'a ObjKey)> {
    if name != "_" {
        methods
            .iter()
            .enumerate()
            .find(|(_i, x)| objs.lobjs[**x].same_id(pkg, name, objs))
    } else {
        None
    }
}

/// missing_method returns None if 't' implements 'intf', otherwise it
/// returns a missing method required by T and whether it is missing or
/// just has the wrong type.
///
/// For non-interface types 't', or if static is set, 't' implements
/// 'intf' if all methods of 'intf' are present in 't'. Otherwise ('t'
/// is an interface and static is not set), missing_method only checks
/// that methods of 'intf' which are also present in 't' have matching
/// types (e.g., for a type assertion x.(T) where x is of
/// interface type 't').
pub fn missing_method(
    t: TypeKey,
    intf: TypeKey,
    static_: bool,
    objs: &TCObjects,
) -> Option<(ObjKey, bool)> {
    let ival = &objs.types[intf].try_as_interface().unwrap();
    if ival.is_empty() {
        return None;
    }
    let tval = objs.types[t].underlying_val(objs);
    if let Some(detail) = tval.try_as_interface() {
        for fkey in ival.all_methods().as_ref().unwrap().iter() {
            let fval = &objs.lobjs[*fkey];
            if let Some((_i, f)) = lookup_method(
                detail.all_methods().as_ref().unwrap(),
                fval.pkg(),
                fval.name(),
                objs,
            ) {
                if !typ::identical_option(fval.typ(), objs.lobjs[*f].typ(), objs) {
                    return Some((*fkey, true));
                }
            } else if static_ {
                return Some((*fkey, false));
            }
        }
        return None;
    }
    // A concrete type implements 'intf' if it implements all methods of 'intf'.
    for fkey in ival.all_methods().as_ref().unwrap().iter() {
        let fval = &objs.lobjs[*fkey];
        match lookup_field_or_method(t, false, fval.pkg(), fval.name(), objs) {
            LookupResult::Entry(okey, _, _) => {
                let result_obj = &objs.lobjs[okey];
                if !result_obj.entity_type().is_func() {
                    return Some((*fkey, false));
                } else if !typ::identical_option(fval.typ(), result_obj.typ(), objs) {
                    return Some((*fkey, true));
                }
            }
            _ => return Some((*fkey, false)),
        }
    }
    None
}

fn lookup_field_or_method_impl(
    tkey: TypeKey,
    addressable: bool,
    pkg: Option<PackageKey>,
    name: &str,
    objs: &TCObjects,
) -> LookupResult {
    if name == "_" {
        return LookupResult::NotFound;
    }
    let (tkey, is_ptr) = try_deref(tkey, objs);
    if is_ptr && typ::is_interface(tkey, objs) {
        // pointer to interface has no methods
        return LookupResult::NotFound;
    }
    // Start with typ as single entry at shallowest depth.
    let mut current = vec![EmbeddedType::new(tkey, None, is_ptr, false)];
    // indices is lazily initialized
    let mut indices = None;
    let mut target = None;
    let mut indirect = false;
    // Named types that we have seen already, allocated lazily.
    // Used to avoid endless searches in case of recursive types.
    // Since only Named types can be used for recursive types, we
    // only need to track those.
    // (If we ever allow type aliases to construct recursive types,
    // we must use type identity rather than pointer equality for
    // the map key comparison, as we do in consolidate_multiples.)
    // seen is lazily initialized
    let mut seen: Option<HashSet<TypeKey>> = None;
    while !current.is_empty() {
        // embedded types found at current depth
        let mut next = vec![];
        for et in current.iter() {
            let mut tobj = &objs.types[et.typ];
            if let typ::Type::Named(detail) = tobj {
                if seen.is_none() {
                    seen = Some(HashSet::new());
                }
                let seen_mut = seen.as_mut().unwrap();
                if seen_mut.contains(&et.typ) {
                    // We have seen this type before, at a more shallow depth
                    // (note that multiples of this type at the current depth
                    // were consolidated before). The type at that depth shadows
                    // this same type at the current depth, so we can ignore
                    // this one.
                    continue;
                }
                seen_mut.insert(et.typ);
                // look for a matching attached method
                if let Some((i, &okey)) = lookup_method(detail.methods(), pkg, name, objs) {
                    lookup_on_found!(indices, i, &mut target, et, indirect, okey);
                    continue; // we can't have a matching field or interface method
                }
                // continue with underlying type
                tobj = &objs.types[detail.underlying()];
            }
            match tobj {
                typ::Type::Struct(detail) => {
                    for (i, &f) in detail.fields().iter().enumerate() {
                        let fobj = &objs.lobjs[f];
                        if fobj.same_id(pkg, name, objs) {
                            lookup_on_found!(indices, i, &mut target, et, indirect, f);
                            continue; // we can't have a matching field or interface method
                        }
                        // Collect embedded struct fields for searching the next
                        // lower depth, but only if we have not seen a match yet
                        // (if we have a match it is either the desired field or
                        // we have a name collision on the same depth; in either
                        // case we don't need to look further).
                        // Embedded fields are always of the form T or *T where
                        // T is a type name. If e.typ appeared multiple times at
                        // this depth, f.typ appears multiple times at the next
                        // depth.
                        if target.is_none() && fobj.var_embedded() {
                            let (tkey, is_ptr) = try_deref(fobj.typ().unwrap(), objs);
                            match &objs.types[tkey] {
                                typ::Type::Named(_)
                                | typ::Type::Struct(_)
                                | typ::Type::Interface(_) => next.push(EmbeddedType::new(
                                    tkey,
                                    concat_vec(et.indices.clone(), i),
                                    et.indirect || is_ptr,
                                    et.multiples,
                                )),
                                _ => {}
                            }
                        }
                    }
                }
                typ::Type::Interface(detail) => {
                    let all = detail.all_methods();
                    if let Some((i, &okey)) = lookup_method(all.as_ref().unwrap(), pkg, name, objs)
                    {
                        lookup_on_found!(indices, i, &mut target, et, indirect, okey);
                    }
                }
                _ => {}
            }
        }
        if let Some(okey) = target {
            // found a potential match
            // spec: "A method call x.m() is valid if the method set of (the type of) x
            //        contains m and the argument list can be assigned to the parameter
            //        list of m. If x is addressable and &x's method set contains m, x.m()
            //        is shorthand for (&x).m()".
            let lobj = &objs.lobjs[okey];
            if lobj.entity_type().is_func() && ptr_recv(lobj, objs) && !indirect && !addressable {
                return LookupResult::BadMethodReceiver;
            }
            return LookupResult::Entry(okey, indices.unwrap(), indirect);
        }
        current = consolidate_multiples(next, objs);
    }
    LookupResult::NotFound
}

/// ptr_recv returns true if the receiver is of the form *T.
fn ptr_recv(lo: &obj::LangObj, objs: &TCObjects) -> bool {
    // If a method's receiver type is set, use that as the source of truth for the receiver.
    // Caution: Checker.func_decl (decl.rs) marks a function by setting its type to an empty
    // signature. We may reach here before the signature is fully set up: we must explicitly
    // check if the receiver is set (we cannot just look for non-None lo.typ).
    if lo.typ().is_none() {
        return false;
    }
    if let Some(sig) = objs.types[lo.typ().unwrap()].try_as_signature() {
        if let Some(re) = sig.recv() {
            let t = objs.lobjs[*re].typ().unwrap();
            let (_, is_ptr) = try_deref(t, objs);
            return is_ptr;
        }
    }
    // If a method's type is not set it may be a method/function that is:
    // 1) client-supplied (via NewFunc with no signature), or
    // 2) internally created but not yet type-checked.
    // For case 1) we can't do anything; the client must know what they are doing.
    // For case 2) we can use the information gathered by the resolver.
    lo.entity_type().func_has_ptr_recv()
}

/// concat_vec returns the result of concatenating list and i.
fn concat_vec(list: Option<Vec<usize>>, i: usize) -> Option<Vec<usize>> {
    if list.is_none() {
        return Some(vec![i]);
    }
    let mut result = list.unwrap();
    result.push(i);
    Some(result)
}

#[derive(Debug)]
struct EmbeddedType {
    typ: TypeKey,
    indices: Option<Vec<usize>>, // lazy init
    indirect: bool,
    multiples: bool,
}

impl EmbeddedType {
    fn new(
        typ: TypeKey,
        indices: Option<Vec<usize>>,
        indirect: bool,
        multiples: bool,
    ) -> EmbeddedType {
        EmbeddedType {
            typ: typ,
            indices: indices,
            indirect: indirect,
            multiples: multiples,
        }
    }
}

/// consolidate_multiples collects multiple list entries with the same type
/// into a single entry marked as containing multiples.
/// it consumes the 'list' and returns a new one
fn consolidate_multiples(list: Vec<EmbeddedType>, objs: &TCObjects) -> Vec<EmbeddedType> {
    let mut result = Vec::with_capacity(list.len());
    if list.len() == 0 {
        return result;
    }
    // lookup finds the identical 'typ' in 'map', returns the value in HashMap
    let lookup = |map: &HashMap<TypeKey, usize>, typ: TypeKey| {
        if let Some(i) = map.get(&typ) {
            Some(*i)
        } else {
            map.iter()
                .find(|(k, _i)| typ::identical(**k, typ, objs))
                .map(|(_k, i)| *i)
        }
    };
    let mut map = HashMap::new();
    for et in list.into_iter() {
        if let Some(i) = lookup(&map, et.typ) {
            result[i].multiples = true;
        } else {
            map.insert(et.typ, result.len());
            result.push(et);
        }
    }
    result
}

/// FieldCollision represents either a field or a name collision
#[derive(Clone, Debug, Eq, PartialEq)]
enum FieldCollision {
    Var(ObjKey),
    Collision,
}

/// add_to_field_set adds field f to the field set s.
/// If multiples is set, f appears multiple times
/// and is treated as a collision.
fn add_to_field_set(
    set: &mut HashMap<String, FieldCollision>,
    f: &ObjKey,
    multiples: bool,
    objs: &TCObjects,
) {
    let key = objs.lobjs[*f].id(objs);
    if !multiples {
        if set
            .insert(key.to_string(), FieldCollision::Var(*f))
            .is_none()
        {
            // no collision
            return;
        }
    }
    set.insert(key.to_string(), FieldCollision::Collision);
}

/// MethodCollision represents either a selection or a name collision
#[derive(Clone, Debug)]
enum MethodCollision {
    Method(Selection),
    Collision,
}

// add_to_method_set adds all functions in list to the method set s.
// If multiples is set, every function in list appears multiple times
// and is treated as a collision.
fn add_to_method_set(
    set: &mut HashMap<String, MethodCollision>,
    list: &Vec<ObjKey>,
    indices: &Vec<usize>,
    indirect: bool,
    multiples: bool,
    objs: &TCObjects,
) {
    for (i, okey) in list.iter().enumerate() {
        let mobj = &objs.lobjs[*okey];
        let key = mobj.id(objs).to_string();
        if !multiples {
            // see the original comment of the Go source in case of a problem
            if !set.contains_key(&key) && (indirect || !ptr_recv(mobj, objs)) {
                set.insert(
                    key,
                    MethodCollision::Method(Selection::new(
                        SelectionKind::MethodVal,
                        None,
                        *okey,
                        concat_vec(Some(indices.clone()), i).unwrap(),
                        indirect,
                        objs,
                    )),
                );
                continue;
            }
        }
        set.insert(key, MethodCollision::Collision);
    }
}
