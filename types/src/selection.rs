use super::objects::{ObjKey, TypeKey};

pub enum SelectionKind {
    FieldVal,   // x.f is a struct field selector
    MethodVal,  // x.f is a method selector
    MethodExpr, // x.f is a method expression
}

pub struct Selection {
    kind: SelectionKind,
    recv: TypeKey,     // type of x
    obj: ObjKey,       // object denoted by x.f
    index: Vec<isize>, // path from x to x.f
    indirect: bool,    // set if there was any pointer indirection on the path
}
