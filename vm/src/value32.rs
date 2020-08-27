/// 32bit GosValue for VM stack to gain performance
///
use super::value::*;
use std::cell::{Ref, RefCell, RefMut};

pub enum Value32Type {
    Bool,
    Int,
    Float64,
    Complex64,
    Str,
    Boxed,
    Closure,
    Slice,
    Map,
    Interface,
    Struct,
    Channel,
    Function,
    Package,
    Meta,
}

union Value32Union {
    ubool: bool,
    uint: isize,
    ufloat64: f64,
    ucomplex64: (f32, f32),
    ustr: *const String,
    uboxed: *const RefCell<GosValue>,
}
