use super::objects::*;
use super::value::{GosValue, RCQueue, RCount, IRC};
use std::cell::RefCell;
use std::convert::TryFrom;
use std::rc::{Rc, Weak};

pub type GcObjs = Vec<GcWeak>;

#[derive(Debug, Clone)]
pub enum GcWeak {
    Array(Weak<(ArrayObj, RCount)>),
    Closure(Weak<(RefCell<ClosureObj>, RCount)>),
    Slice(Weak<(SliceObj, RCount)>),
    Map(Weak<(MapObj, RCount)>),
    Interface(Weak<(RefCell<InterfaceObj>, RCount)>),
    Struct(Weak<(RefCell<StructObj>, RCount)>),
    Channel(Weak<(RefCell<ChannelObj>, RCount)>),
}

impl GcWeak {
    pub fn from_gosv(v: &GosValue) -> GcWeak {
        match v {
            GosValue::Array(a) => GcWeak::Array(Rc::downgrade(a)),
            GosValue::Closure(c) => GcWeak::Closure(Rc::downgrade(c)),
            GosValue::Slice(s) => GcWeak::Slice(Rc::downgrade(s)),
            GosValue::Map(m) => GcWeak::Map(Rc::downgrade(m)),
            GosValue::Interface(i) => GcWeak::Interface(Rc::downgrade(i)),
            GosValue::Struct(s) => GcWeak::Struct(Rc::downgrade(s)),
            GosValue::Channel(c) => GcWeak::Channel(Rc::downgrade(c)),
            _ => unreachable!(),
        }
    }

    fn to_gosv(&self) -> Option<GosValue> {
        match &self {
            GcWeak::Array(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Array(v)
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
        }
    }
}

fn children_ref_sub_one(val: &GosValue) {
    match val {
        GosValue::Array(arr) => arr
            .0
            .borrow_data_mut()
            .iter()
            .for_each(|obj| obj.borrow().ref_sub_one()),
        GosValue::Closure(c) => c.0.borrow().ref_sub_one(),
        GosValue::Slice(s) => {
            let sdata = &s.0;
            if !sdata.is_nil() {
                sdata
                    .borrow_data()
                    .iter()
                    .for_each(|obj| obj.borrow().ref_sub_one())
            }
        }
        GosValue::Map(m) => {
            let mdata = &m.0;
            if !mdata.is_nil() {
                mdata.borrow_data().iter().for_each(|(k, v)| {
                    k.ref_sub_one();
                    v.borrow().ref_sub_one();
                })
            }
        }
        GosValue::Interface(i) => i.0.borrow().ref_sub_one(),
        GosValue::Struct(s) => s.0.borrow().fields.iter().for_each(|obj| obj.ref_sub_one()),
        GosValue::Channel(_) => unimplemented!(),
        _ => unreachable!(),
    };
}

fn children_mark_dirty(val: &GosValue, queue: &mut RCQueue) {
    match val {
        GosValue::Array(arr) => arr
            .0
            .borrow_data_mut()
            .iter()
            .for_each(|obj| obj.borrow().mark_dirty(queue)),
        GosValue::Closure(c) => c.0.borrow().mark_dirty(queue),
        GosValue::Slice(s) => {
            let sdata = &s.0;
            if !sdata.is_nil() {
                sdata
                    .borrow_data()
                    .iter()
                    .for_each(|obj| obj.borrow().mark_dirty(queue))
            }
        }
        GosValue::Map(m) => {
            let mdata = &m.0;
            if !mdata.is_nil() {
                mdata.borrow_data().iter().for_each(|(k, v)| {
                    k.mark_dirty(queue);
                    v.borrow().mark_dirty(queue);
                })
            }
        }
        GosValue::Interface(i) => i.0.borrow().mark_dirty(queue),
        GosValue::Struct(s) => {
            s.0.borrow()
                .fields
                .iter()
                .for_each(|obj| obj.mark_dirty(queue))
        }
        GosValue::Channel(_) => unimplemented!(),
        _ => unreachable!(),
    };
}

fn release(obj: &mut GosValue) {
    match obj {
        GosValue::Array(arr) => arr.0.borrow_data_mut().clear(),
        GosValue::Closure(c) => {
            let r: &mut ClosureObj = &mut RefCell::borrow_mut(&c.0);
            if let Some(uvs) = &mut r.uvs {
                uvs.clear();
            }
            r.recv = None;
            r.ffi = None;
        }
        GosValue::Slice(s) => {
            let sdata = &s.0;
            if !sdata.is_nil() {
                sdata.borrow_data_mut().clear();
            }
        }
        GosValue::Map(m) => {
            let mdata = &m.0;
            if !mdata.is_nil() {
                mdata.borrow_data_mut().clear();
            }
        }
        GosValue::Interface(i) => RefCell::borrow_mut(&i.0).set_underlying(IfaceUnderlying::None),
        GosValue::Struct(s) => RefCell::borrow_mut(&s.0).fields.clear(),
        GosValue::Channel(_) => unimplemented!(),
        _ => unreachable!(),
    };
}

/// put the non-zero-rc on the left, and the others on the right
fn partition_to_scan(to_scan: &mut Vec<GosValue>) -> usize {
    let len = to_scan.len();
    if len == 0 {
        return 0;
    }
    let mut p0 = 0;
    let mut p1 = len - 1;
    loop {
        while p0 < len - 1 && to_scan[p0].rc() > 0 {
            p0 += 1;
        }
        while p1 > 1 && to_scan[p1].rc() <= 0 {
            p1 -= 1;
        }
        if p0 >= p1 {
            break;
        }
        to_scan.swap(p0, p1);
    }
    p0
}

pub fn gc(objs: &mut GcObjs) {
    let mut to_scan: Vec<GosValue> = objs.iter().filter_map(|o| o.to_gosv()).collect();
    for v in to_scan.iter() {
        println!("{}", v);
        dbg!(v.rc());
    }
    dbg!(to_scan.len());
    for v in to_scan.iter() {
        children_ref_sub_one(v);
    }

    let boundary = partition_to_scan(&mut to_scan);
    for i in boundary..to_scan.len() {
        to_scan[i].set_rc(-(i as IRC));
    }

    let mut queue: RCQueue = RCQueue::new();
    for i in 0..boundary {
        children_mark_dirty(&to_scan[i], &mut queue);
    }

    loop {
        if let Some(i) = queue.pop_front() {
            let obj = &to_scan[(-i) as usize];
            obj.set_rc(666);
            children_mark_dirty(&obj, &mut queue);
        } else {
            break;
        }
    }

    for mut obj in to_scan.into_iter() {
        if obj.rc() <= 0 {
            release(&mut obj);
        }
    }

    let result: Vec<GosValue> = objs.iter().filter_map(|o| o.to_gosv()).collect();
    for v in result.iter() {
        println!("{}", v);
        //dbg!(&v);
        dbg!(v.rc());
    }

    dbg!(result.len());
}
