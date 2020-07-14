#![allow(dead_code)]
use super::objects::{ObjKey, TCObjects, TypeKey};
use super::typ;
use std::fmt;
use std::fmt::Write;

/// SelectionKind describes the kind of a selector expression x.f
/// (excluding qualified identifiers).
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct Selection {
    kind: SelectionKind,
    recv: Option<TypeKey>, // type of x
    obj: ObjKey,           // object denoted by x.f
    indices: Vec<usize>,   // path from x to x.f
    indirect: bool,        // set if there was any pointer indirection on the path
    typ: Option<TypeKey>,  // evaluation delayed
    id: Option<String>,    // evaluation delayed
}

impl Selection {
    pub fn new(
        kind: SelectionKind,
        recv: Option<TypeKey>,
        obj: ObjKey,
        indices: Vec<usize>,
        indirect: bool,
    ) -> Selection {
        Selection {
            kind: kind,
            recv: recv,
            obj: obj,
            indices: indices,
            indirect: indirect,
            typ: None,
            id: None,
        }
    }

    pub fn init(&mut self, objs: &mut TCObjects) {
        self.typ = Some(self.eval_type(objs));
        self.id = Some(objs.lobjs[self.obj].id(objs).to_string());
    }

    pub fn kind(&self) -> &SelectionKind {
        &self.kind
    }

    pub fn recv(&self) -> Option<TypeKey> {
        self.recv
    }

    pub fn obj(&self) -> ObjKey {
        self.obj
    }

    /// typ must be called after init is called
    pub fn typ(&self) -> TypeKey {
        self.typ.unwrap()
    }

    /// id must be called after init is called
    pub fn id(&self) -> &String {
        self.id.as_ref().unwrap()
    }

    /// indices describes the path from x to f in x.f.
    /// The last indices entry is the field or method indices of the type declaring f;
    /// either:
    ///
    ///	1) the list of declared methods of a named type; or
    ///	2) the list of methods of an interface type; or
    ///	3) the list of fields of a struct type.
    ///
    /// The earlier indices entries are the indices of the embedded fields implicitly
    /// traversed to get from (the type of) x to f, starting at embedding depth 0.
    pub fn indices(&self) -> &Vec<usize> {
        &self.indices
    }

    /// Indirect reports whether any pointer indirection was required to get from
    /// x to f in x.f.
    pub fn indirect(&self) -> &bool {
        &self.indirect
    }

    pub fn fmt(&self, f: &mut fmt::Formatter<'_>, objs: &TCObjects) -> fmt::Result {
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
                typ::fmt_type(Some(self.typ()), f, objs)?;
            }
            _ => typ::fmt_signature(self.typ(), f, objs)?,
        }
        Ok(())
    }

    /// eval_type evaluates the type of x.f, which may be different from the type of f.
    /// See Selection for more information.
    fn eval_type(&self, objs: &mut TCObjects) -> TypeKey {
        let obj = &objs.lobjs[self.obj];
        match self.kind {
            SelectionKind::FieldVal => obj.typ().unwrap(),
            SelectionKind::MethodVal => {
                let t = &objs.types[obj.typ().unwrap()];
                let mut sig = *t.try_as_signature().unwrap();
                let mut new_recv = objs.lobjs[sig.recv().unwrap()].clone();
                new_recv.set_type(self.recv);
                sig.set_recv(Some(objs.lobjs.insert(new_recv)));
                objs.types.insert(typ::Type::Signature(sig))
            }
            SelectionKind::MethodExpr => {
                let t = &objs.types[obj.typ().unwrap()];
                let mut sig = *t.try_as_signature().unwrap();
                let mut arg0 = objs.lobjs[sig.recv().unwrap()].clone();
                arg0.set_type(self.recv);
                let arg0key = objs.lobjs.insert(arg0);
                sig.set_recv(None);
                let mut params = vec![arg0key];
                let tup = &mut objs.types[sig.params()].try_as_tuple_mut().unwrap();
                params.append(&mut tup.vars().clone());
                sig.set_params(objs.new_t_tuple(params));
                objs.types.insert(typ::Type::Signature(sig))
            }
        }
    }
}
