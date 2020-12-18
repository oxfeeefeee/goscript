//#![allow(dead_code)]
//use super::opcode::OpIndex;
use super::instruction::*;
use super::value::{VMObjects, GosValue, RuntimeResult};
use super::stack::Stack;

macro_rules! upframe {
    ($iter:expr, $f:expr) => {
        $iter.find(|x| x.func() == $f).unwrap();
    };
}

macro_rules! read_imm_pkg {
    ($code:ident, $frame:ident, $objs:ident) => {{
        let inst = $code[$frame.pc];
        $frame.pc += 1;
        u64_to_key(inst.get_u64())
    }};
}

macro_rules! store_local {
    ($stack:ident, $s_index:expr, $rhs_index:expr, $typ:expr) => {{
        if $rhs_index < 0 {
            let rhs_s_index = Stack::offset($stack.len(), $rhs_index);
            $stack.store_copy_semantic($s_index, rhs_s_index, $typ);
        } else {
            let op_ex = Instruction::index2code($rhs_index);
            $stack.store_with_op($s_index, $stack.len() - 1, op_ex, $typ);
        }   
    }};
}

macro_rules! load_up_value {
    ($upvalue:expr, $self_:ident, $stack:ident, $frame:ident) => {{
        let uv: &UpValueState = &$upvalue.inner.borrow();
        match &uv {
            UpValueState::Open(desc) => {
                $stack.get_with_type(desc.index as usize, desc.typ)
            }
            UpValueState::Closed(val) => {
                val.clone()
            }
        }
    }};
}

macro_rules! store_up_value {
    ($upvalue:expr, $self_:ident, $stack:ident, $frame:ident, $rhs_index:ident, $typ:expr) => {{
        let uv: &mut UpValueState = &mut $upvalue.inner.borrow_mut();
        match uv {
            UpValueState::Open(desc) => {
                store_local!($stack, desc.index as usize, $rhs_index, $typ);
            }
            UpValueState::Closed(v) => {
                $stack.store_val(v, $rhs_index, $typ);
            }
        }
    }};
}

macro_rules! deref_value {
    ($pointers:expr, $self_:ident, $stack:ident, $frame:ident, $objs:expr) => {{
        match $pointers {
            GosValue::Pointer(b) => {
                match *b {
                    PointerObj::UpVal(uv) => load_up_value!(&uv, $self_, $stack, $frame),
                    PointerObj::LocalRefType(s, md) => match md {
                        GosMetadata::Untyped => GosValue::Struct(s),
                        _ => GosValue::Named(Box::new((GosValue::Struct(s), md))),
                    }
                    PointerObj::SliceMember(s, index) => s.get(index as usize).unwrap(),
                    PointerObj::StructField(s, index) => s.borrow().fields[index as usize].clone(),
                    PointerObj::PkgMember(pkg, index) => $objs.packages[pkg].member(index).clone(),
                }
            }
            _ => unreachable!(),
        }
    }};
}

macro_rules! range_vars {
    ($m_ref:ident, $m_ptr:ident, $m_iter:ident, $l_ref:ident, $l_ptr:ident, $l_iter:ident,
        $s_ref:ident, $s_ptr:ident, $s_iter:ident) => {
        let mut $m_ref: Option<Rc<RefCell<GosHashMap>>> = None;
        let mut $m_ptr: Option<*mut Ref<GosHashMap>> = None;
        let mut $m_iter: Option<std::collections::hash_map::Iter<GosValue, RefCell<GosValue>>> = None;
        let mut $l_ref: Option<Rc<SliceObj>> = None;
        let mut $l_ptr: Option<*mut SliceRef> = None;
        let mut $l_iter: Option<SliceEnumIter> = None;
        let mut $s_ref: Option<Rc<StringObj>> = None;
        let mut $s_ptr: Option<*mut Rc<StringObj>> = None;
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
                            drop(Box::<Rc<StringObj>>::from_raw(p));
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
        _ => {
            dbg!(val);
            unreachable!();
        }
    }
}

#[inline]
pub fn load_field(val: &GosValue, ind: &GosValue, objs: &VMObjects) -> GosValue {
    match val {
        GosValue::Struct(sval) => {
            match &ind {
                GosValue::Int(i) => {
                    sval.borrow().fields[*i as usize].clone()
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
pub fn store_index(stack: &Stack, target: &GosValue, key: &GosValue, r_index: OpIndex, t: ValueType) {
    match target {
        GosValue::Slice(s) => {
            let target_cell = &s.borrow_data()[*key.as_int() as usize];
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t);
        }
        GosValue::Map(map) => {
            map.touch_key(&key);
            let borrowed = map.borrow_data();
            let target_cell = borrowed.get(&key).unwrap();
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t);   
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn store_index_int(stack: &Stack, target: &GosValue, i: usize, r_index: OpIndex, t: ValueType) -> RuntimeResult {
    match target {
        GosValue::Slice(s) => {
            let target_cell = &s.borrow_data()[i];
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t);
            Ok(())
        }
        GosValue::Map(map) => {
            let key = GosValue::Int(i as isize);
            map.touch_key(&key);
            let borrowed = map.borrow_data();
            let target_cell = borrowed.get(&key).unwrap();
            stack.store_val(&mut target_cell.borrow_mut(), r_index, t);   
            Ok(())
        }
        GosValue::Nil(_) => {
            Err("assignment to entry in nil map or slice".to_string())
        }
        _ => {
            dbg!(target);
            unreachable!()
        }
    }
}

#[inline]
pub fn store_field(stack: &Stack, target: &GosValue, key: &GosValue, r_index: OpIndex, t: ValueType, objs: &VMObjects) {
    match target {
        GosValue::Struct(s) => {
            match key {
                GosValue::Int(i) => {
                    let target = &mut s.borrow_mut().fields[*i as usize];
                    stack.store_val(target, r_index, t);
                }
                GosValue::Str(sval) => {
                    let i =s.borrow().meta.field_index(sval.as_str(), &objs.metas);
                    let target = &mut s.borrow_mut().fields[i as usize];
                    stack.store_val(target, r_index, t);
                }
                _ => unreachable!(),
            };
        }
        _ => unreachable!(),
    }
}

#[inline]
pub fn push_index_comma_ok(stack: &mut Stack, map: &GosValue, index: &GosValue) {
    let (v, b) = match map.as_map().try_get(index) {
        Some(v) => (v, true),
        None => (GosValue::new_nil(), false),
    };
    stack.push(v);
    stack.push_bool(b);
}
