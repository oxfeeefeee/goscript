use super::objects::*;
use super::value::{GosValue, RCQueue, RCount, IRC};
use std::cell::Ref;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::rc::{Rc, Weak};

pub struct GcoVec {
    inner: Rc<RefCell<Vec<GcWeak>>>,
}

impl GcoVec {
    pub fn new() -> GcoVec {
        GcoVec {
            inner: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[inline]
    pub fn add(&self, v: &GosValue) {
        let weak = GcWeak::from_gosv(v);
        self.add_weak(weak);
    }

    #[inline]
    pub fn add_weak(&self, w: GcWeak) {
        self.inner.borrow_mut().push(w);
    }

    fn borrow_data(&self) -> Ref<Vec<GcWeak>> {
        self.inner.borrow()
    }
}

#[derive(Debug, Clone)]
pub enum GcWeak {
    Array(Weak<(ArrayObj, RCount)>),
    Closure(Weak<(RefCell<ClosureObj>, RCount)>),
    Slice(Weak<(SliceObj, RCount)>),
    Map(Weak<(MapObj, RCount)>),
    Struct(Weak<(RefCell<StructObj>, RCount)>),
    // todo:
    // GC doesn't support channel for now, because we can't access the
    // underlying objects (unless we don't use smol::channel and write
    // it ourself).
    // but memory leaking circles that involve channels should be very
    // rare, so in practice we may not need it at all.
    //Channel(Weak<(RefCell<ChannelObj>, RCount)>),
}

impl GcWeak {
    pub fn from_gosv(v: &GosValue) -> GcWeak {
        match v {
            GosValue::Array(a) => GcWeak::Array(Rc::downgrade(a)),
            GosValue::Closure(c) => GcWeak::Closure(Rc::downgrade(c)),
            GosValue::Slice(s) => GcWeak::Slice(Rc::downgrade(s)),
            GosValue::Map(m) => GcWeak::Map(Rc::downgrade(m)),
            GosValue::Struct(s) => GcWeak::Struct(Rc::downgrade(s)),
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
            GcWeak::Struct(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::Struct(v)
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
            // todo: slice shares data, could use some optimization
            let sdata = &s.0;
            if !sdata.is_nil() {
                sdata
                    .borrow()
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
        GosValue::Struct(s) => s.0.borrow().fields.iter().for_each(|obj| obj.ref_sub_one()),
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
            // todo: slice shares data, could use some optimization
            if !sdata.is_nil() {
                sdata
                    .borrow()
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
        GosValue::Struct(s) => {
            s.0.borrow()
                .fields
                .iter()
                .for_each(|obj| obj.mark_dirty(queue))
        }
        _ => unreachable!(),
    };
}

fn break_cycle(obj: &mut GosValue) {
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
                // slices share data, we cannot clear the data here
            }
        }
        GosValue::Map(m) => {
            let mdata = &m.0;
            if !mdata.is_nil() {
                mdata.borrow_data_mut().clear();
            }
        }
        GosValue::Struct(s) => RefCell::borrow_mut(&s.0).fields.clear(),
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

pub fn gc(objs: &GcoVec) {
    let mut to_scan: Vec<GosValue> = objs
        .borrow_data()
        .iter()
        .filter_map(|o| o.to_gosv())
        .collect();
    //print!("objs before GC: {}\n", to_scan.len());
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
            break_cycle(&mut obj);
        }
    }

    let _result: Vec<GosValue> = objs
        .borrow_data()
        .iter()
        .filter_map(|o| o.to_gosv())
        .collect();
    //print!("objs left after GC: {}\n", result.len());
}
