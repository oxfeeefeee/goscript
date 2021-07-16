use super::metadata::GosMetadata;
use super::objects::*;
use super::value::{GosValue, RCount};
use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::rc::{Rc, Weak};

pub type GcObjs = Vec<GcWeak>;

#[derive(Debug, Clone)]
pub enum GcWeak {
    Array(Weak<(ArrayObj, RCount)>),
    Pointer(Weak<(RefCell<PointerObj>, RCount)>),
    Closure(Weak<(RefCell<ClosureObj>, RCount)>),
    Slice(Weak<(SliceObj, RCount)>),
    Map(Weak<(MapObj, RCount)>),
    Interface(Weak<(RefCell<InterfaceObj>, RCount)>),
    Struct(Weak<(RefCell<StructObj>, RCount)>),
    Channel(Weak<(RefCell<ChannelObj>, RCount)>),
    Named(Weak<(GosValue, GosMetadata, RCount)>),
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

    fn to_gosv(&self) -> Option<GosValue> {
        match &self {
            GcWeak::Array(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Array(v)
            }),
            GcWeak::Pointer(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Pointer(v)
            }),
            GcWeak::Closure(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Closure(v)
            }),
            GcWeak::Slice(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Slice(v)
            }),
            GcWeak::Map(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Map(v)
            }),
            GcWeak::Interface(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Interface(v)
            }),
            GcWeak::Struct(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Struct(v)
            }),
            GcWeak::Channel(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Channel(v)
            }),
            GcWeak::Named(w) => w.upgrade().map(|v| {
                v.2.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Named(v)
            }),
        }
    }

    fn release(&self) {
        match &self {
            GcWeak::Array(w) => {
                if let Some(s) = w.upgrade() {
                    s.0.borrow_data_mut().clear();
                }
            }
            GcWeak::Pointer(w) => {
                if let Some(s) = w.upgrade() {
                    let r: &mut PointerObj = &mut (*s).0.borrow_mut();
                    *r = PointerObj::Released;
                }
            }
            GcWeak::Closure(w) => {
                if let Some(s) = w.upgrade() {
                    let r: &mut ClosureObj = &mut RefCell::borrow_mut(&s.0);
                    if let Some(uvs) = &mut r.uvs {
                        uvs.clear();
                    }
                    r.recv = None;
                    r.ffi = None;
                }
            }
            GcWeak::Slice(w) => {
                if let Some(mut s) = w.upgrade() {
                    s.borrow_mut().0.borrow_data_mut().clear();
                }
            }
            GcWeak::Map(w) => {
                if let Some(mut s) = w.upgrade() {
                    s.borrow_mut().0.borrow_data_mut().clear();
                }
            }
            GcWeak::Interface(w) => {
                if let Some(s) = w.upgrade() {
                    RefCell::borrow_mut(&s.0).set_underlying(IfaceUnderlying::None);
                }
            }
            GcWeak::Struct(w) => {
                if let Some(s) = w.upgrade() {
                    RefCell::borrow_mut(&s.0).fields.clear();
                }
            }
            GcWeak::Channel(_) => unimplemented!(),
            GcWeak::Named(w) => {
                if let Some(s) = w.upgrade() {
                    match s.0.clone() {
                        GosValue::Array(a) => a.0.borrow_data_mut().clear(),
                        GosValue::Pointer(p) => *p.0.borrow_mut() = PointerObj::Released,
                        GosValue::Slice(slice) => slice.0.borrow_data_mut().clear(),
                        GosValue::Map(m) => m.0.borrow_data_mut().clear(),
                        GosValue::Interface(i) => {
                            RefCell::borrow_mut(&i.0).set_underlying(IfaceUnderlying::None)
                        }
                        GosValue::Struct(s) => RefCell::borrow_mut(&s.0).fields.clear(),
                        _ => unreachable!(),
                    }
                }
            }
        };
    }
}

fn children_ref_sub_one(val: &GosValue) {
    match val {
        GosValue::Array(arr) => arr
            .0
            .borrow_data_mut()
            .iter()
            .for_each(|obj| obj.borrow().ref_sub_one()),
        GosValue::Pointer(p) => p.0.borrow().ref_sub_one(),
        GosValue::Closure(c) => c.0.borrow().ref_sub_one(),
        GosValue::Slice(s) => {
            s.0.borrow_data_mut()
                .iter()
                .for_each(|obj| obj.borrow().ref_sub_one())
        }
        GosValue::Map(m) => m.0.borrow_data().iter().for_each(|(k, v)| {
            k.ref_sub_one();
            v.borrow().ref_sub_one();
        }),
        GosValue::Interface(i) => i.0.borrow().ref_sub_one(),
        GosValue::Struct(s) => s.0.borrow().fields.iter().for_each(|obj| obj.ref_sub_one()),
        GosValue::Channel(_) => unimplemented!(),
        GosValue::Named(n) => n.0.ref_sub_one(),
        _ => unreachable!(),
    };
}

pub fn gc(objs: &mut GcObjs) {
    let mut to_scan: Vec<GosValue> = objs.iter().filter_map(|o| o.to_gosv()).collect();

    for v in to_scan.iter() {
        //println!("{}", v);
        dbg!(v.rc());
    }
    for v in to_scan.iter() {
        children_ref_sub_one(v);
    }
    dbg!("==========");
    for v in to_scan.iter() {
        println!("{}", v);
        dbg!(v.rc());
    }
    //dbg!(&to_scan);
    dbg!(objs.len());
    dbg!(&to_scan.len());
}
