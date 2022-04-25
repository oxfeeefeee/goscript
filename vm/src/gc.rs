use super::instruction::ValueType;
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

    pub fn add_array(&self, arr: &Rc<(GosArrayObj, RCount)>) {
        self.add_weak(GcWeak::new_array(arr))
    }

    pub fn add_closure(&self, cls: &Rc<(ClosureObj, RCount)>) {
        self.add_weak(GcWeak::new_closure(cls))
    }

    pub fn add_map(&self, m: &Rc<(MapObj, RCount)>) {
        self.add_weak(GcWeak::new_map(m))
    }

    pub fn add_struct(&self, s: &Rc<(StructObj, RCount)>) {
        self.add_weak(GcWeak::new_struct(s))
    }

    #[inline]
    pub fn add_weak(&self, w: GcWeak) {
        self.inner.borrow_mut().push(w);
    }

    fn borrow_data(&self) -> Ref<Vec<GcWeak>> {
        self.inner.borrow()
    }
}

#[derive(Clone)]
pub enum GcWeak {
    Array(Weak<(GosArrayObj, RCount)>),
    Closure(Weak<(ClosureObj, RCount)>),
    Map(Weak<(MapObj, RCount)>),
    Struct(Weak<(StructObj, RCount)>),
    // todo:
    // GC doesn't support channel for now, because we can't access the
    // underlying objects (unless we don't use smol::channel and write
    // it ourself).
    // but memory leaking circles that involve channels should be very
    // rare, so in practice we may not need it at all.
    //Channel(Weak<(RefCell<ChannelObj>, RCount)>),
}

impl GcWeak {
    pub fn new_array(arr: &Rc<(GosArrayObj, RCount)>) -> GcWeak {
        GcWeak::Array(Rc::downgrade(arr))
    }

    pub fn new_closure(cls: &Rc<(ClosureObj, RCount)>) -> GcWeak {
        GcWeak::Closure(Rc::downgrade(cls))
    }

    pub fn new_map(m: &Rc<(MapObj, RCount)>) -> GcWeak {
        GcWeak::Map(Rc::downgrade(m))
    }

    pub fn new_struct(s: &Rc<(StructObj, RCount)>) -> GcWeak {
        GcWeak::Struct(Rc::downgrade(s))
    }

    fn to_gosv(&self) -> Option<GosValue> {
        match &self {
            GcWeak::Array(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::from_gos_array(v)
            }),
            GcWeak::Closure(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::from_closure(Some(v))
            }),
            GcWeak::Map(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::from_map(Some(v))
            }),
            GcWeak::Struct(w) => w.upgrade().map(|v| {
                v.1.set(i32::try_from(w.strong_count()).unwrap() - 1);
                GosValue::from_struct(v)
            }),
        }
    }
}

fn children_ref_sub_one(val: &GosValue) {
    match val.typ() {
        ValueType::Array => val
            .as_gos_array()
            .0
            .borrow_data_mut()
            .iter()
            .for_each(|obj| obj.ref_sub_one()),
        ValueType::Closure => match val.as_closure() {
            Some(cls) => cls.0.ref_sub_one(),
            None => {}
        },
        ValueType::Map => match val.as_map() {
            Some(m) => m.0.borrow_data().iter().for_each(|(k, v)| {
                k.ref_sub_one();
                v.ref_sub_one();
            }),
            None => {}
        },
        ValueType::Struct => val
            .as_struct()
            .0
            .borrow_fields()
            .iter()
            .for_each(|obj| obj.ref_sub_one()),
        _ => unreachable!(),
    };
}

fn children_mark_dirty(val: &GosValue, queue: &mut RCQueue) {
    match val.typ() {
        ValueType::Array => val
            .as_gos_array()
            .0
            .borrow_data_mut()
            .iter()
            .for_each(|obj| obj.mark_dirty(queue)),
        ValueType::Closure => match val.as_closure() {
            Some(cls) => cls.0.mark_dirty(queue),
            None => {}
        },
        ValueType::Map => match val.as_map() {
            Some(m) => m.0.borrow_data().iter().for_each(|(k, v)| {
                k.mark_dirty(queue);
                v.mark_dirty(queue);
            }),
            None => {}
        },
        ValueType::Struct => val
            .as_struct()
            .0
            .borrow_fields()
            .iter()
            .for_each(|obj| obj.mark_dirty(queue)),
        _ => unreachable!(),
    };
}

/// break_cycle clears all values held by containers
fn break_cycle(val: &GosValue) {
    match val.typ() {
        ValueType::Array => val.as_gos_array().0.borrow_data_mut().clear(),
        ValueType::Map => match val.as_map() {
            Some(m) => m.0.borrow_data_mut().clear(),
            None => {}
        },
        ValueType::Struct => val.as_struct().0.borrow_fields_mut().clear(),
        ValueType::Closure => {}
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

    for obj in to_scan.into_iter() {
        if obj.rc() <= 0 {
            break_cycle(&obj);
        }
    }

    let _result: Vec<GosValue> = objs
        .borrow_data()
        .iter()
        .filter_map(|o| o.to_gosv())
        .collect();
    //print!("objs left after GC: {}\n", result.len());
}
