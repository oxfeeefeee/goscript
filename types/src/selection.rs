#![allow(dead_code)]
use super::objects::{ObjKey, TCObjects, TypeKey};
use super::typ;
use std::fmt;
use std::fmt::Write;

/// SelectionKind describes the kind of a selector expression x.f
/// (excluding qualified identifiers).
pub enum SelectionKind {
    FieldVal,   // x.f is a struct field selector
    MethodVal,  // x.f is a method selector
    MethodExpr, // x.f is a method expression
}

/// A Selection describes a selector expression x.f.
/// For the declarations:
///
///	type T struct{ x int; E }
///	type E struct{}
///	func (e E) m() {}
///	var p *T
///
/// the following relations exist:
///
///	Selector    Kind          Recv    Obj    Type               Index     Indirect
///
///	p.x         FieldVal      T       x      int                {0}       true
///	p.m         MethodVal     *T      m      func (e *T) m()    {1, 0}    true
///	T.m         MethodExpr    T       m      func m(_ T)        {1, 0}    false
///
pub struct Selection {
    kind: SelectionKind,
    recv: Option<TypeKey>, // type of x
    obj: ObjKey,           // object denoted by x.f
    index: Vec<isize>,     // path from x to x.f
    indirect: bool,        // set if there was any pointer indirection on the path
    typ: TypeKey,          // lazy evaluated type
}

impl Selection {
    pub fn new(
        kind: SelectionKind,
        recv: Option<TypeKey>,
        obj: ObjKey,
        index: Vec<isize>,
        indirect: bool,
        objs: &mut TCObjects,
    ) -> Selection {
        let typ = Selection::init_type(&obj, &kind, &recv, objs);
        Selection {
            kind: kind,
            recv: recv,
            obj: obj,
            index: index,
            indirect: indirect,
            typ: typ,
        }
    }

    pub fn kind(&self) -> &SelectionKind {
        &self.kind
    }

    pub fn recv(&self) -> &Option<TypeKey> {
        &self.recv
    }

    pub fn obj(&self) -> &ObjKey {
        &self.obj
    }

    pub fn typ(&self) -> &TypeKey {
        &self.typ
    }

    /// Index describes the path from x to f in x.f.
    /// The last index entry is the field or method index of the type declaring f;
    /// either:
    ///
    ///	1) the list of declared methods of a named type; or
    ///	2) the list of methods of an interface type; or
    ///	3) the list of fields of a struct type.
    ///
    /// The earlier index entries are the indices of the embedded fields implicitly
    /// traversed to get from (the type of) x to f, starting at embedding depth 0.
    pub fn index(&self) -> &Vec<isize> {
        &self.index
    }

    /// Indirect reports whether any pointer indirection was required to get from
    /// x to f in x.f.
    pub fn indirect(&self) -> &bool {
        &self.indirect
    }

    /// init_type evaluates the type of x.f, which may be different from the type of f.
    /// See Selection for more information.
    fn init_type(
        obj: &ObjKey,
        kind: &SelectionKind,
        recv: &Option<TypeKey>,
        objs: &mut TCObjects,
    ) -> TypeKey {
        let obj = &objs.lobjs[*obj];
        match kind {
            SelectionKind::FieldVal => obj.typ().unwrap(),
            SelectionKind::MethodVal => {
                let t = &objs.types[obj.typ().unwrap()];
                let mut sig = *t.try_as_signature().unwrap();
                let mut new_recv = objs.lobjs[sig.recv().unwrap()].clone();
                new_recv.set_type(*recv);
                sig.set_recv(Some(objs.lobjs.insert(new_recv)));
                objs.types.insert(typ::Type::Signature(sig))
            }
            SelectionKind::MethodExpr => {
                let t = &objs.types[obj.typ().unwrap()];
                let mut sig = *t.try_as_signature().unwrap();
                let mut arg0 = objs.lobjs[sig.recv().unwrap()].clone();
                arg0.set_type(*recv);
                let arg0key = objs.lobjs.insert(arg0);
                sig.set_recv(None);
                let mut params = vec![arg0key];
                if let Some(tkey) = sig.params() {
                    let tup = &mut objs.types[*tkey].try_as_tuple_mut().unwrap();
                    params.append(tup.vars_mut())
                }
                let params_tuple = typ::TupleDetail::new(params);
                sig.set_params(Some(objs.types.insert(typ::Type::Tuple(params_tuple))));
                objs.types.insert(typ::Type::Signature(sig))
            }
        }
    }

    pub fn fmt_selection(&self, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
        f.write_str(match self.kind {
            SelectionKind::FieldVal => "field (",
            SelectionKind::MethodVal => "method (",
            SelectionKind::MethodExpr => "method expr (",
        })?;
        typ::fmt_type(self.recv(), f, objs)?;
        write!(f, ") {}", objs.lobjs[self.obj].name())?;
        match self.kind {
            SelectionKind::FieldVal => {
                f.write_char(' ')?;
                typ::fmt_type(&Some(*self.typ()), f, objs)?;
            }
            _ => typ::fmt_signature(self.typ(), f, objs)?,
        }
        Ok(())
    }
}
