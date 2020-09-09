//#![allow(dead_code)]
//use super::opcode::OpIndex;
use super::instruction::*;
use super::value::{VMObjects, GosValue, ClosureVal};
use std::cell::RefCell;
use std::rc::Rc;
use super::stack::Stack;

macro_rules! upframe {
    ($iter:expr, $f:expr) => {
        $iter.find(|x| x.callable.func() == $f).unwrap();
    };
}

macro_rules! bind_method {
    ($sval:ident, $val:ident, $index:ident, $objs:ident) => {
        GosValue::Closure(
            Rc::new(RefCell::new(ClosureVal {
                func: $objs.metas[*$sval.borrow().meta.as_meta()].typ()
                    .get_struct_member($index)
                    .as_closure().borrow().func,
                receiver: Some($val.clone()),
                upvalues: vec![],
            })),
        )
    };
}

macro_rules! range_vars {
    ($m_ref:ident, $m_ptr:ident, $m_iter:ident, $l_ref:ident, $l_ptr:ident, $l_iter:ident,
        $s_ref:ident, $s_ptr:ident, $s_iter:ident) => {
        let mut $m_ref: Option<Rc<RefCell<GosHashMap>>> = None;
        let mut $m_ptr: Option<*mut Ref<GosHashMap>> = None;
        let mut $m_iter: Option<std::collections::hash_map::Iter<GosValue, RefCell<GosValue>>> = None;
        let mut $l_ref: Option<Rc<SliceVal>> = None;
        let mut $l_ptr: Option<*mut SliceRef> = None;
        let mut $l_iter: Option<SliceEnumIter> = None;
        let mut $s_ref: Option<Rc<StringVal>> = None;
        let mut $s_ptr: Option<*mut Rc<StringVal>> = None;
        let mut $s_iter: Option<StringEnumIter> = None;
    };
}

macro_rules! range_init {
    ($objs:ident, $target:ident, $map_ref:ident, $map_ptr:ident, $map_iter:ident,
        $slice_ref:ident, $slice_ptr:ident, $slice_iter:ident, 
        $str_ref:ident, $str_ptr:ident, $str_iter:ident) => {{
        match &$target {
            GosValue::Map(m) => {
                let map = m.clone_inner();
                $map_ref.replace(map);
                let r = $map_ref.as_ref().unwrap().borrow();
                let p = Box::into_raw(Box::new(r));
                $map_ptr = Some(p);
                let mapref = unsafe { p.as_ref().unwrap() };
                $map_iter = Some(mapref.iter());
            }
            GosValue::Slice(sl) => {
                let slice = sl.clone();
                $slice_ref.replace(slice);
                let r = $slice_ref.as_ref().unwrap().borrow();
                let p = Box::into_raw(Box::new(r));
                $slice_ptr = Some(p);
                let sliceref = unsafe { p.as_ref().unwrap() };
                $slice_iter = Some(sliceref.iter().enumerate());
            }
            GosValue::Str(s) => {
                let string = s.clone();
                $str_ref.replace(string);
                let r = $str_ref.as_ref().unwrap().clone();
                let p = Box::into_raw(Box::new(r));
                $str_ptr = Some(p);
                let strref = unsafe { p.as_ref().unwrap() };
                $str_iter = Some(strref.iter().enumerate());
            }
            _ => unreachable!(),
        }
    }};
}

macro_rules! range_body {
    ($target:expr, $stack:ident, $inst:ident,
        $map_ptr:ident, $map_iter:ident,
        $slice_ptr:ident, $slice_iter:ident,
        $str_ptr:ident, $str_iter:ident) => {{
        match &$target {
            GosValue::Map(_) => {
                let v = $map_iter.as_mut().unwrap().next();
                if let Some((k, v)) = v {
                    $stack.push(k.clone());
                    $stack.push(v.clone().into_inner());
                    false
                } else {
                    $map_iter.take();
                    // release the pointer
                    if let Some(p) = $map_ptr {
                        unsafe {
                            drop(Box::<Ref<GosHashMap>>::from_raw(p));
                        }
                        $map_ptr = None
                    }
                    $stack.pop_discard();
                    $stack.pop_discard();
                    true
                }
            }
            GosValue::Slice(_) => {
                let v = $slice_iter.as_mut().unwrap().next();
                if let Some((k, v)) = v {
                    $stack.push_int(k as isize);
                    $stack.push(v.clone().into_inner());
                    false
                } else {
                    $slice_iter.take();
                    // release the pointer
                    if let Some(p) = $slice_ptr {
                        unsafe {
                            drop(Box::<SliceRef>::from_raw(p));
                        }
                        $slice_ptr = None
                    }
                    $stack.pop_discard();
                    $stack.pop_discard();
                    true
                }
            }
            GosValue::Str(_) => {
                let v = $str_iter.as_mut().unwrap().next();
                if let Some((k, v)) = v {
                    $stack.push_int(k as isize);
                    $stack.push_int(v as isize);
                    false
                } else {
                    $str_iter.take();
                    // release the pointer
                    if let Some(p) = $str_ptr {
                        unsafe {
                            drop(Box::<Rc<StringVal>>::from_raw(p));
                        }
                        $str_ptr = None
                    }
                    $stack.pop_discard();
                    $stack.pop_discard();
                    true
                }
            }
            _ => unreachable!(),
        }
    }};
}

#[inline]
pub fn load_index(val: &GosValue, ind: &GosValue) -> GosValue {
    match val {
        GosValue::Slice(slice) => {
            let index = *ind.as_int() as usize;
            if let Some(v) = slice.get(index) {
                v
            } else {
                unimplemented!();
            }
        }
        GosValue::Map(map) => map.get(&ind).clone(),
        GosValue::Str(s) => {
            GosValue::Int(s.get_byte(*ind.as_int() as usize) as isize)
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn load_index_int(val: &GosValue, i: usize) -> GosValue {
    match val {
        GosValue::Slice(slice) => {
            if let Some(v) = slice.get(i) {
                v
            } else {
                unimplemented!();
            }
        }
        GosValue::Map(map) => {
            let ind = GosValue::Int(i as isize);
            map.get(&ind).clone()
        }
        GosValue::Str(s) => {
            GosValue::Int(s.get_byte(i) as isize)
        }

        _ => unreachable!(),
    }
}

#[inline]
pub fn load_field(val: &GosValue, ind: &GosValue, objs: &VMObjects) -> GosValue {
    match val {
        GosValue::Struct(sval) => {
            match &ind {
                GosValue::Int(i) => sval.borrow().fields[*i as usize].clone(),
                GosValue::Str(s) => {
                    let struct_ref = sval.borrow();
                    let index = struct_ref.field_method_index(s.as_str(), &objs.metas);
                    if index < struct_ref.fields.len() as OpIndex {
                        struct_ref.fields[index as usize].clone()
                    } else {
                        bind_method!(sval, val, index, objs)
                    }
                }
                _ => unreachable!(),
            }
        }
        GosValue::Package(pkey) => {
            let pkg = &objs.packages[*pkey];
            pkg.member(*ind.as_int() as OpIndex).clone()
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn store_index(stack: &Stack, target: &GosValue, key: &GosValue, r_index: OpIndex, t: ValueType, objs: &VMObjects) {
    match target {
        GosValue::Slice(s) => {
            let target_cell = &s.borrow_data()[*key.as_int() as usize];
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t, &objs.zero_val);
        }
        GosValue::Map(map) => {
            map.touch_key(&key);
            let borrowed = map.borrow_data();
            let target_cell = borrowed.get(&key).unwrap();
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t, &objs.zero_val);   
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn store_index_int(stack: &Stack, target: &GosValue, i: usize, r_index: OpIndex, t: ValueType, objs: &VMObjects) {
    match target {
        GosValue::Slice(s) => {
            let target_cell = &s.borrow_data()[i];
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t, &objs.zero_val);
        }
        GosValue::Map(map) => {
            let key = GosValue::Int(i as isize);
            map.touch_key(&key);
            let borrowed = map.borrow_data();
            let target_cell = borrowed.get(&key).unwrap();
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t, &objs.zero_val);   
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn store_field(stack: &Stack, target: &GosValue, key: &GosValue, r_index: OpIndex, t: ValueType, objs: &VMObjects) {
    match target {
        GosValue::Struct(s) => {
            match key {
                GosValue::Int(i) => {
                    let target = &mut s.borrow_mut().fields[*i as usize];
                    stack.store_val(target, r_index, t, &objs.zero_val);
                }
                GosValue::Str(sval) => {
                    let i = s.borrow().field_method_index(sval.as_str(), &objs.metas);
                    let target = &mut s.borrow_mut().fields[i as usize];
                    stack.store_val(target, r_index, t, &objs.zero_val);
                }
                _ => unreachable!(),
            };
        }
        _ => unreachable!(),
    }
}
