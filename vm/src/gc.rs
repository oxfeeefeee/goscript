use super::metadata::GosMetadata;
use super::objects::*;
use super::value::GosValue;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

#[derive(Debug)]
enum GcWeak {
    Array(Weak<ArrayObj>),
    Pointer(Weak<PointerObj>),
    Closure(Weak<ClosureObj>),
    Slice(Weak<SliceObj>),
    Map(Weak<MapObj>),
    Interface(Weak<RefCell<InterfaceObj>>),
    Struct(Weak<RefCell<StructObj>>),
    Channel(Weak<RefCell<ChannelObj>>),
    Named(Weak<(GosValue, GosMetadata)>),
}

#[derive(Debug)]
pub struct GcObj {
    weak: GcWeak,
    rc: i32,
}

impl GcObj {
    pub fn from_gosv(v: &GosValue) -> GcObj {
        let weak = match v {
            GosValue::Array(a) => GcWeak::Array(Rc::downgrade(a)),
            GosValue::Pointer(p) => GcWeak::Pointer(Rc::downgrade(p)),
            GosValue::Closure(c) => GcWeak::Closure(Rc::downgrade(c)),
            GosValue::Slice(s) => GcWeak::Slice(Rc::downgrade(s)),
            GosValue::Map(m) => GcWeak::Map(Rc::downgrade(m)),
            GosValue::Interface(i) => GcWeak::Interface(Rc::downgrade(i)),
            GosValue::Struct(s) => GcWeak::Struct(Rc::downgrade(s)),
            GosValue::Channel(c) => GcWeak::Channel(Rc::downgrade(c)),
            GosValue::Named(n) => GcWeak::Named(Rc::downgrade(n)),
            _ => unreachable!(),
        };
        GcObj { weak: weak, rc: 0 }
    }
}
