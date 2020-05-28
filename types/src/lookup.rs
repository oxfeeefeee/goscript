#![allow(dead_code)]
use super::obj;
use super::objects::{ObjKey, PackageKey, TCObjects, TypeKey};
use super::selection::*;
use super::typ;
use std::collections::{HashMap, HashSet};

macro_rules! lookup_on_found {
    ($indices:ident, $i:ident, $target:ident, $et:ident, $indirect:ident, $found:expr) => {
        $indices = concat_vec($indices, $i);
        if $target.is_some() || $et.multiples {
            return LookupResult::Ambiguous($indices.unwrap());
        }
        $target = Some($found);
        $indirect = $et.indirect;
    };
}

/// the result of lookup_field_or_method
pub enum LookupResult {
    /// valid entry
    Entry(ObjKey, Vec<usize>),
    /// the index sequence points to an ambiguous entry
    /// (the same name appeared more than once at the same embedding level).
    Ambiguous(Vec<usize>),
    /// a method with a pointer receiver type was found
    /// but there was no pointer on the path from the actual receiver type to
    /// the method's formal receiver base type, nor was the receiver addressable.
    Indirect,
    /// nothing found
    NotFound,
}

pub struct MethodSet {
    list: Vec<Selection>,
}

impl MethodSet {
    /*pub fn new(t: &TypeKey, objs: &TCObjects) -> MethodSet {}

    pub fn list(&self) -> &Vec<Selection> {
        &self.list
    }

    pub fn lookup(pkg: &PackageKey, name: &str, objs: &TCObjects) -> Option<&Selection> {}*/
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
    tkey: &TypeKey,
    addressable: bool,
    pkg: &Option<PackageKey>,
    name: &str,
    objs: &TCObjects,
) -> LookupResult {
    if let Some(named) = &objs.types[*tkey].try_as_named() {
        // Methods cannot be associated to a named pointer type
        // (spec: "The type denoted by T is called the receiver base type;
        // it must not be a pointer or interface type and it must be declared
        // in the same package as the method.").
        // Thus, if we have a named pointer type, proceed with the underlying
        // pointer type but discard the result if it is a method since we would
        // not have found it for T
        let pkey = named.underlying();
        if objs.types[*pkey].try_as_pointer().is_some() {
            let re = lookup_field_or_method_impl(pkey, false, pkg, name, objs);
            if let LookupResult::Entry(okey, _) = &re {
                if objs.lobjs[*okey].entity_type().is_func() {
                    return LookupResult::NotFound;
                }
            }
            return re;
        }
    }
    lookup_field_or_method_impl(tkey, addressable, pkg, name, objs)
}

/// deref dereferences typ if it is a *Pointer and returns its base.
/// Otherwise it returns None.
pub fn try_deref<'a>(t: &'a TypeKey, objs: &'a TCObjects) -> (&'a TypeKey, bool) {
    match &objs.types[*t] {
        typ::Type::Pointer(detail) => (detail.base(), true),
        _ => (t, false),
    }
}

/// deref_struct_ptr dereferences typ if it is a (named or unnamed) pointer to a
/// (named or unnamed) struct and returns its base. Otherwise it returns typ.   
pub fn deref_struct_ptr<'a>(t: &'a TypeKey, objs: &'a TCObjects) -> &'a TypeKey {
    let ut = typ::underlying_type(t, objs);
    match &objs.types[*ut] {
        typ::Type::Pointer(detail) => {
            let but = typ::underlying_type(detail.base(), objs);
            match &objs.types[*but] {
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
    pkg: &Option<PackageKey>,
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
    pkg: &Option<PackageKey>,
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

fn lookup_field_or_method_impl(
    tkey: &TypeKey,
    addressable: bool,
    pkg: &Option<PackageKey>,
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
    let mut current = vec![EmbeddedType::new(*tkey, None, is_ptr, false)];
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
        let mut next = vec![];
        for et in current.iter() {
            let mut tobj = &objs.types[et.typ];
            if let typ::Type::Named(detail) = tobj {
                if seen.is_none() {
                    seen = Some(HashSet::new());
                }
                let seen_mut = seen.as_mut().unwrap();
                if seen_mut.get(&et.typ).is_some() {
                    // We have seen this type before, at a more shallow depth
                    // (note that multiples of this type at the current depth
                    // were consolidated before). The type at that depth shadows
                    // this same type at the current depth, so we can ignore
                    // this one.
                    continue;
                }
                seen_mut.insert(et.typ);
                // look for a matching attached method
                if let Some((i, okey)) = lookup_method(detail.methods(), pkg, name, objs) {
                    lookup_on_found!(indices, i, target, et, indirect, okey);
                    continue; // we can't have a matching field or interface method
                }
                // continue with underlying type
                tobj = &objs.types[*detail.underlying()];
            }
            match tobj {
                typ::Type::Struct(detail) => {
                    for (i, f) in detail.fields().iter().enumerate() {
                        let fobj = &objs.lobjs[*f];
                        if fobj.same_id(pkg, name, objs) {
                            lookup_on_found!(indices, i, target, et, indirect, f);
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
                        if target.is_none() && *fobj.var_embedded() {
                            let (tkey, is_ptr) = try_deref(fobj.typ().as_ref().unwrap(), objs);
                            match &objs.types[*tkey] {
                                typ::Type::Named(_)
                                | typ::Type::Struct(_)
                                | typ::Type::Interface(_) => next.push(EmbeddedType::new(
                                    *tkey,
                                    concat_vec(indices.clone(), i),
                                    et.indirect || is_ptr,
                                    et.multiples,
                                )),
                                _ => {}
                            }
                        }
                    }
                }
                typ::Type::Interface(detail) => {
                    if let Some((i, okey)) = lookup_method(detail.methods(), pkg, name, objs) {
                        lookup_on_found!(indices, i, target, et, indirect, okey);
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
            let lobj = &objs.lobjs[*okey];
            if lobj.entity_type().is_func() && ptr_recv(lobj, objs) && !indirect && !addressable {
                return LookupResult::Indirect;
            }
            return LookupResult::Entry(*okey, indices.unwrap());
        }
        current = consolidate_multiples(next, objs);
    }
    LookupResult::NotFound
}

/// ptr_recv reports whether the receiver is of the form *T.
/// The receiver must exist.
fn ptr_recv(lo: &obj::LangObj, objs: &TCObjects) -> bool {
    let recv = objs.types[lo.typ().unwrap()]
        .try_as_signature()
        .unwrap()
        .recv();
    let t = objs.lobjs[recv.unwrap()].typ().as_ref().unwrap();
    let (_, is_ptr) = try_deref(t, objs);
    is_ptr
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
    let lookup = |map: &HashMap<TypeKey, usize>, typ: &TypeKey| {
        if let Some(i) = map.get(typ) {
            Some(*i)
        } else {
            map.iter()
                .find(|(k, _i)| typ::identical(*k, typ, objs))
                .map(|(_k, i)| *i)
        }
    };
    let mut map = HashMap::new();
    for et in list.into_iter() {
        if let Some(i) = lookup(&map, &et.typ) {
            result[i].multiples = true;
        } else {
            map.insert(et.typ, result.len());
            result.push(et);
        }
    }
    result
}
