use super::metadata::GosMetadata;
use super::objects::*;
use super::value::GosValue;
use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub type GcObjs = Vec<GcWeak>;

#[derive(Debug)]
pub enum GcWeak {
    Array(Weak<ArrayObj>),
    Pointer(Weak<RefCell<PointerObj>>),
    Closure(Weak<RefCell<ClosureObj>>),
    Slice(Weak<SliceObj>),
    Map(Weak<MapObj>),
    Interface(Weak<RefCell<InterfaceObj>>),
    Struct(Weak<RefCell<StructObj>>),
    Channel(Weak<RefCell<ChannelObj>>),
    Named(Weak<(GosValue, GosMetadata)>),
}

impl GcWeak {
    pub fn from_gosv(v: &GosValue) -> GcWeak {
        match v {
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
        }
    }

    fn read_rc(&mut self) -> usize {
        match &self {
            GcWeak::Array(w) => {
                if let Some(arr) = w.upgrade() {
                    let c = w.strong_count();
                    c
                } else {
                    0
                }
            }
            GcWeak::Pointer(w) => w.strong_count(),
            GcWeak::Closure(w) => w.strong_count(),
            GcWeak::Slice(w) => w.strong_count(),
            GcWeak::Map(w) => w.strong_count(),
            GcWeak::Interface(w) => w.strong_count(),
            GcWeak::Struct(w) => w.strong_count(),
            GcWeak::Channel(w) => w.strong_count(),
            GcWeak::Named(w) => w.strong_count(),
        }
    }

    fn release(&self) {
        match &self {
            GcWeak::Array(w) => {
                if let Some(s) = w.upgrade() {
                    s.borrow_data_mut().clear();
                }
            }
            GcWeak::Pointer(w) => {
                if let Some(s) = w.upgrade() {
                    let r: &mut PointerObj = &mut (*s).borrow_mut();
                    *r = PointerObj::Released;
                }
            }
            GcWeak::Closure(w) => {
                if let Some(s) = w.upgrade() {
                    let r: &mut ClosureObj = &mut RefCell::borrow_mut(&s);
                    if let Some(uvs) = &mut r.uvs {
                        uvs.clear();
                    }
                    r.recv = None;
                    r.ffi = None;
                }
            }
            GcWeak::Slice(w) => {
                if let Some(mut s) = w.upgrade() {
                    s.borrow_mut().borrow_data_mut().clear();
                }
            }
            GcWeak::Map(w) => {
                if let Some(mut s) = w.upgrade() {
                    s.borrow_mut().borrow_data_mut().clear();
                }
            }
            GcWeak::Interface(w) => {
                if let Some(s) = w.upgrade() {
                    RefCell::borrow_mut(&s).set_underlying(IfaceUnderlying::None);
                }
            }
            GcWeak::Struct(w) => {
                if let Some(s) = w.upgrade() {
                    RefCell::borrow_mut(&s).fields.clear();
                }
            }
            GcWeak::Channel(_) => unimplemented!(),
            GcWeak::Named(w) => {
                if let Some(s) = w.upgrade() {
                    match s.0.clone() {
                        GosValue::Array(a) => a.borrow_data_mut().clear(),
                        GosValue::Pointer(p) => *(*p).borrow_mut() = PointerObj::Released,
                        GosValue::Slice(mut slice) => slice.borrow_mut().borrow_data_mut().clear(),
                        GosValue::Map(mut m) => m.borrow_mut().borrow_data_mut().clear(),
                        GosValue::Interface(i) => {
                            RefCell::borrow_mut(&i).set_underlying(IfaceUnderlying::None)
                        }
                        GosValue::Struct(s) => RefCell::borrow_mut(&s).fields.clear(),
                        _ => unreachable!(),
                    }
                }
            }
        };
    }
}

pub struct TestD {
    pub a: Rc<(RefCell<i8>, i8)>,
}

pub fn gc(objs: &mut GcObjs) {
    for o in objs.iter_mut() {
        dbg!(o.read_rc());
    }
    dbg!(objs.len());

    let t = TestD {
        a: Rc::new((RefCell::<i8>::new(0), 0)),
    };
    let mut b = t.a.0.borrow_mut();
    *b = 2;
}
