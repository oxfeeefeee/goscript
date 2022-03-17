#![allow(dead_code)]
use super::gc::GcoVec;
use super::instruction::{Instruction, OpIndex, Opcode, ValueType};
use super::metadata::GosMetadata;
use super::objects::ValueDesc;
use super::value::*;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt::{self, Display};
use std::mem;
use std::ptr;
use std::rc::Rc;

const DEFAULT_SIZE: usize = 10240;

macro_rules! stack_binary_op {
    ($stack:ident, $op:tt, $t:ident) => {{
        let len = $stack.len();
        let a = $stack.get_c(len - 2);
        let b = $stack.get_c(len - 1);
        *$stack.get_c_mut(len - 2) = GosValue64::$op(a, b, $t);
        $stack.pop_discard();
    }};
}

macro_rules! stack_cmp_op {
    ($stack:ident, $op:tt, $t:ident) => {{
        let len = $stack.len();
        let a = $stack.get_c(len - 2);
        let b = $stack.get_c(len - 1);
        *$stack.get_c_mut(len - 2) = GosValue64::from_bool(GosValue64::$op(a, b, $t));
        $stack.pop_discard();
    }};
}

macro_rules! store_to_copy_semantic {
    ($from:expr,
        $to:expr,
        $li:expr,
        $ri:expr,
        $t:expr,
        $gcos:expr) => {{
        if $t.copyable() {
            *$to.get_c_mut($li) = *$from.get_c($ri);
        } else {
            let v = $from.get_rc($ri).copy_semantic($gcos);
            *$to.get_rc_mut($li) = v;
        }
    }};
}

macro_rules! store_to_with_op {
    ($from:expr,
        $to:expr,
        $li:expr,
        $ri:expr,
        $op:expr,
        $t:expr) => {{
        if $t.copyable() {
            let a = $from.get_c($li);
            let b = $from.get_c($ri);
            *$to.get_c_mut($li) = GosValue64::binary_op(a, b, $t, $op);
        } else {
            let a = $from.get_rc($li);
            let b = $from.get_rc($ri);
            *$to.get_rc_mut($li) = GosValue::add_str(a, b);
        }
    }};
}

pub struct Stack {
    c: Vec<GosValue64>,
    rc: Vec<GosValue>,
    cursor: usize,
    max: usize,
}

impl Display for Stack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Debug for Stack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[stack] cursor: {}\n", self.cursor)?;
        f.write_str("======c  top=======\n")?;
        for i in 0..(self.cursor + 1) {
            write!(f, "{}\n", self.c[i].get_uint())?;
        }
        f.write_str("======c  botton====\n")?;
        f.write_str("=====rc  top=======\n")?;
        for i in 0..(self.cursor + 1) {
            write!(f, "{}\n", &self.rc[i])?;
        }
        f.write_str("=====rc  botton====\n")
    }
}

impl Stack {
    pub fn new() -> Stack {
        Stack {
            c: vec![GosValue64::nil(); DEFAULT_SIZE],
            rc: vec![GosValue::new_nil(); DEFAULT_SIZE],
            cursor: 0,
            max: DEFAULT_SIZE - 1,
        }
    }

    pub fn with_data(mut c: Vec<GosValue64>, mut rc: Vec<GosValue>) -> Stack {
        let n = c.len();
        debug_assert!(n == rc.len());
        let size_to_go = DEFAULT_SIZE - n;
        c.append(&mut vec![GosValue64::nil(); size_to_go]);
        rc.append(&mut vec![GosValue::new_nil(); size_to_go]);
        Stack {
            c: c,
            rc: rc,
            cursor: n,
            max: DEFAULT_SIZE - 1,
        }
    }

    pub fn move_from(other: &mut Stack, count: usize) -> Stack {
        let (c, rc) = other.pop_n(count);
        Stack::with_data(c, rc)
    }

    pub fn pop_n(&mut self, n: usize) -> (Vec<GosValue64>, Vec<GosValue>) {
        let begin = self.cursor - n;
        let end = self.cursor;
        self.cursor -= n;
        let mut c: Vec<GosValue64> = vec![GosValue64::nil(); n];
        c.copy_from_slice(&self.c[begin..end]);
        (
            c,
            self.rc[begin..end]
                .iter_mut()
                .map(|x| std::mem::replace(x, GosValue::new_nil()))
                .collect(),
        )
    }

    pub fn push_n(&mut self, c: Vec<GosValue64>, rc: Vec<GosValue>) {
        let n = c.len();
        debug_assert!(n == rc.len());
        let begin = self.cursor;
        let end = begin + n;
        self.c[begin..end].copy_from_slice(&c[0..n]);
        for (i, v) in rc.into_iter().enumerate() {
            self.rc[begin + i] = v;
        }
        self.cursor += n;
    }

    #[inline]
    pub fn push(&mut self, val: GosValue) {
        match GosValue64::from_v128(&val) {
            Some(v) => {
                *self.get_c_mut(self.cursor) = v;
            }
            None => {
                *self.get_rc_mut(self.cursor) = val;
            }
        }
        self.cursor += 1;
        assert!(self.cursor <= self.max); //todo: expand
    }

    #[inline]
    pub fn push_from_index(&mut self, index: usize, t: ValueType) {
        if t.copyable() {
            *self.get_c_mut(self.cursor) = *self.get_c(index);
        } else {
            *self.get_rc_mut(self.cursor) = self.get_rc(index).clone();
        }
        self.cursor += 1;
        assert!(self.cursor <= self.max); //todo: expand
    }

    #[inline]
    pub fn push_nil(&mut self) {
        *self.get_rc_mut(self.cursor) = GosValue::new_nil();
        self.cursor += 1;
        assert!(self.cursor <= self.max); //todo: expand
    }

    #[inline]
    pub fn push_bool(&mut self, b: bool) {
        *self.get_c_mut(self.cursor) = GosValue64::from_bool(b);
        self.cursor += 1;
        assert!(self.cursor <= self.max); //todo: expand
    }

    #[inline]
    pub fn push_int(&mut self, i: isize) {
        *self.get_c_mut(self.cursor) = GosValue64::from_int(i);
        self.cursor += 1;
        assert!(self.cursor <= self.max); //todo: expand
    }

    #[inline]
    pub fn push_int32_as(&mut self, i: i32, t: ValueType) {
        *self.get_c_mut(self.cursor) = GosValue64::from_int32_as(i, t);
        self.cursor += 1;
        assert!(self.cursor <= self.max); //todo: expand
    }

    #[inline]
    pub fn pop_discard(&mut self) {
        self.cursor -= 1;
    }

    #[inline]
    pub fn pop_discard_n(&mut self, n: usize) {
        self.cursor -= n;
    }

    #[inline]
    pub fn pop_c(&mut self) -> GosValue64 {
        self.cursor -= 1;
        self.get_c(self.cursor).clone()
    }

    #[inline]
    pub fn pop_rc(&mut self) -> GosValue {
        self.cursor -= 1;
        let mut ret = GosValue::new_nil();
        std::mem::swap(self.get_rc_mut(self.cursor), &mut ret);
        ret
    }

    #[inline]
    pub fn pop_with_type(&mut self, t: ValueType) -> GosValue {
        if t.copyable() {
            self.cursor -= 1;
            self.get_c(self.cursor).get_v128(t)
        } else {
            self.pop_rc()
        }
    }

    #[inline]
    pub fn pop_with_type_n(&mut self, types: &[ValueType]) -> Vec<GosValue> {
        let len = types.len();
        let mut ret = Vec::with_capacity(len);
        for (i, t) in types.iter().enumerate() {
            let index = self.cursor - types.len() + i;
            let val = if t.copyable() {
                self.get_c(index).get_v128(*t)
            } else {
                let mut v = GosValue::new_nil();
                std::mem::swap(self.get_rc_mut(index), &mut v);
                v
            };
            ret.push(val);
        }
        self.cursor -= len;
        ret
    }

    #[inline]
    pub fn pop_interface(&mut self) -> Rc<RefCell<InterfaceObj>> {
        self.cursor -= 1;
        let mut ret = GosValue::new_nil();
        std::mem::swap(self.get_rc_mut(self.cursor), &mut ret);
        match ret {
            GosValue::Interface(i) => i,
            GosValue::Named(n) => match &n.0 {
                GosValue::Interface(i) => i.clone(),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn pop_bool(&mut self) -> bool {
        self.cursor -= 1;
        self.get_c(self.cursor).get_bool()
    }

    #[inline]
    pub fn pop_int(&mut self) -> isize {
        self.cursor -= 1;
        self.get_c(self.cursor).get_int()
    }

    #[inline]
    pub fn pop_int32(&mut self) -> i32 {
        self.cursor -= 1;
        self.get_c(self.cursor).get_int32()
    }

    #[inline]
    pub fn pop_uint(&mut self) -> usize {
        self.cursor -= 1;
        self.get_c(self.cursor).get_uint()
    }

    #[inline]
    pub fn pop_uint32(&mut self) -> u32 {
        self.cursor -= 1;
        self.get_c(self.cursor).get_uint32()
    }

    #[inline]
    pub fn get_with_type(&self, index: usize, t: ValueType) -> GosValue {
        if t.copyable() {
            self.get_c(index).get_v128(t)
        } else {
            self.get_rc(index).clone()
        }
    }

    #[inline]
    pub fn set(&mut self, index: usize, val: GosValue) {
        match GosValue64::from_v128(&val) {
            Some(v) => {
                *self.get_c_mut(index) = v;
            }
            None => {
                *self.get_rc_mut(index) = val;
            }
        }
    }

    #[inline]
    pub fn set_with_type(&mut self, index: usize, val: GosValue64, t: ValueType) {
        if t.copyable() {
            *self.get_c_mut(index) = val;
        } else {
            *self.get_rc_mut(index) = val.get_v128(t)
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.cursor
    }

    #[inline]
    pub fn truncate(&mut self, len: usize) {
        assert!(len <= self.cursor);
        self.cursor = len
    }

    #[inline]
    pub fn append(&mut self, mut vec: Vec<GosValue>) {
        for v in vec.drain(..) {
            self.push(v);
        }
    }

    #[inline]
    pub fn split_off_with_type(&mut self, index: usize, t: ValueType) -> Vec<GosValue> {
        let end = self.cursor;
        self.cursor = index;
        if t.copyable() {
            self.c[index..end].iter().map(|x| x.get_v128(t)).collect()
        } else {
            self.rc[index..end].to_vec()
        }
    }

    #[inline]
    pub fn store_to_copy_semantic(
        from: &Stack,
        to: &mut Stack,
        li: usize,
        ri: usize,
        t: ValueType,
        gcos: &GcoVec,
    ) {
        store_to_copy_semantic!(from, to, li, ri, t, gcos);
    }

    #[inline]
    pub fn store_to_with_op(
        from: &Stack,
        to: &mut Stack,
        li: usize,
        ri: usize,
        op: Opcode,
        t: ValueType,
    ) {
        store_to_with_op!(from, to, li, ri, op, t);
    }

    #[inline]
    pub fn store_copy_semantic(&mut self, li: usize, ri: usize, t: ValueType, gcos: &GcoVec) {
        store_to_copy_semantic!(self, self, li, ri, t, gcos)
    }

    #[inline]
    pub fn store_with_op(&mut self, li: usize, ri: usize, op: Opcode, t: ValueType) {
        store_to_with_op!(self, self, li, ri, op, t);
    }

    #[inline]
    pub fn store_val(&self, target: &mut GosValue, r_index: OpIndex, t: ValueType, gcos: &GcoVec) {
        let val = if r_index < 0 {
            let rhs_s_index = Stack::offset(self.len(), r_index);
            if t.copyable() {
                self.get_c(rhs_s_index).get_v128(t)
            } else {
                self.get_rc(rhs_s_index).copy_semantic(gcos)
            }
        } else {
            let ri = Stack::offset(self.len(), -1);
            let op = Instruction::index2code(r_index);
            if t.copyable() {
                let a = GosValue64::from_v128(target).unwrap();
                let b = self.get_c(ri);
                let v = GosValue64::binary_op(&a, b, t, op);
                v.get_v128(t)
            } else {
                GosValue::add_str(target, self.get_rc(ri))
            }
        };
        *target = val;
    }

    #[inline]
    pub fn pack_variadic(&mut self, index: usize, meta: GosMetadata, t: ValueType, gcos: &GcoVec) {
        if index <= self.len() {
            let mut v = Vec::new();
            v.append(&mut self.split_off_with_type(index, t));
            self.push(GosValue::slice_with_val(v, meta, gcos))
        }
    }

    #[inline]
    pub fn load_upvalue(&self, desc: &ValueDesc) -> GosValue {
        let index = (desc.stack_base + desc.index) as usize;
        let uv_stack = desc.stack.upgrade().unwrap();
        if ptr::eq(uv_stack.as_ptr(), self) {
            self.get_with_type(index, desc.typ)
        } else {
            uv_stack.borrow().get_with_type(index, desc.typ)
        }
    }

    #[inline]
    pub fn init_pkg_vars(&mut self, pkg: &PackageVal, count: usize) {
        for i in 0..count {
            let var_index = (count - 1 - i) as OpIndex;
            let var: &mut GosValue = &mut pkg.var_mut(var_index);
            let t = var.get_type();
            *var = self.pop_with_type(t);
        }
    }

    #[inline]
    pub fn add(&mut self, t: ValueType) {
        if t.copyable() {
            stack_binary_op!(self, binary_op_add, t)
        } else {
            let a = self.get_rc(self.len() - 2);
            let b = self.get_rc(self.len() - 1);
            *self.get_rc_mut(self.len() - 2) = GosValue::add_str(a, b);
            self.pop_discard();
        }
    }

    #[inline]
    pub fn switch_cmp(&mut self, t: ValueType, objs: &VMObjects) -> bool {
        let b = if t.copyable() {
            let len = self.len();
            let a = self.get_c(len - 2);
            let b = self.get_c(len - 1);
            GosValue64::compare_eql(a, b, t)
        } else {
            let a = self.get_rc(self.len() - 2);
            let b = self.get_rc(self.len() - 1);
            if t != ValueType::Metadata {
                a.eq(&b)
            } else {
                a.as_meta().identical(b.as_meta(), &objs.metas)
            }
        };
        self.pop_discard();
        b
    }

    #[inline]
    pub fn sub(&mut self, t: ValueType) {
        stack_binary_op!(self, binary_op_sub, t)
    }

    #[inline]
    pub fn mul(&mut self, t: ValueType) {
        stack_binary_op!(self, binary_op_mul, t)
    }

    #[inline]
    pub fn quo(&mut self, t: ValueType) {
        stack_binary_op!(self, binary_op_quo, t)
    }

    #[inline]
    pub fn rem(&mut self, t: ValueType) {
        stack_binary_op!(self, binary_op_rem, t)
    }

    #[inline]
    pub fn and(&mut self, t: ValueType) {
        stack_binary_op!(self, binary_op_and, t)
    }

    #[inline]
    pub fn or(&mut self, t: ValueType) {
        stack_binary_op!(self, binary_op_or, t)
    }

    #[inline]
    pub fn xor(&mut self, t: ValueType) {
        stack_binary_op!(self, binary_op_xor, t)
    }

    #[inline]
    pub fn shl(&mut self, t0: ValueType, t1: ValueType) {
        let mut right = self.pop_c();
        right.to_uint32(t1);
        self.get_c_mut(self.len() - 1)
            .binary_op_shl(right.get_uint32(), t0);
    }

    #[inline]
    pub fn shr(&mut self, t0: ValueType, t1: ValueType) {
        let mut right = self.pop_c();
        right.to_uint32(t1);
        self.get_c_mut(self.len() - 1)
            .binary_op_shr(right.get_uint32(), t0);
    }

    #[inline]
    pub fn and_not(&mut self, t: ValueType) {
        stack_binary_op!(self, binary_op_and_not, t)
    }

    #[inline]
    pub fn unary_negate(&mut self, t: ValueType) {
        self.get_c_mut(self.len() - 1).unary_negate(t);
    }

    #[inline]
    pub fn unary_xor(&mut self, t: ValueType) {
        self.get_c_mut(self.len() - 1).unary_xor(t);
    }

    #[inline]
    pub fn logical_not(&mut self, t: ValueType) {
        self.get_c_mut(self.len() - 1).unary_not(t);
    }

    #[inline]
    pub fn compare_eql(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_eql, t);
        } else {
            let (b, a) = (self.pop_with_type(t), self.pop_with_type(t));
            self.push_bool(a.eq(&b));
        }
    }

    #[inline]
    pub fn compare_neq(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_neq, t);
        } else {
            let (b, a) = (self.pop_with_type(t), self.pop_with_type(t));
            self.push_bool(!a.eq(&b));
        }
    }

    #[inline]
    pub fn compare_lss(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_lss, t);
        } else {
            let (b, a) = (self.pop_with_type(t), self.pop_with_type(t));
            self.push_bool(a.cmp(&b) == Ordering::Less);
        }
    }

    #[inline]
    pub fn compare_gtr(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_gtr, t);
        } else {
            let (b, a) = (self.pop_with_type(t), self.pop_with_type(t));
            self.push_bool(a.cmp(&b) == Ordering::Greater);
        }
    }

    #[inline]
    pub fn compare_leq(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_leq, t);
        } else {
            let (b, a) = (self.pop_with_type(t), self.pop_with_type(t));
            self.push_bool(a.cmp(&b) != Ordering::Greater);
        }
    }

    #[inline]
    pub fn compare_geq(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_geq, t);
        } else {
            let (b, a) = (self.pop_with_type(t), self.pop_with_type(t));
            self.push_bool(a.cmp(&b) != Ordering::Less);
        }
    }

    #[inline]
    pub fn get_c(&self, i: usize) -> &GosValue64 {
        unsafe { self.c.get_unchecked(i) }
    }

    #[inline]
    pub fn get_c_mut(&mut self, i: usize) -> &mut GosValue64 {
        unsafe { self.c.get_unchecked_mut(i) }
    }

    #[inline]
    pub fn get_rc(&self, i: usize) -> &GosValue {
        unsafe { self.rc.get_unchecked(i) }
    }

    #[inline]
    pub fn get_rc_mut(&mut self, i: usize) -> &mut GosValue {
        unsafe { self.rc.get_unchecked_mut(i) }
    }

    #[inline]
    pub fn offset(base: usize, offset: OpIndex) -> usize {
        (base as isize + offset as isize) as usize
    }

    #[inline]
    pub fn copy_to_rc(&mut self, t: ValueType) {
        *self.get_rc_mut(self.cursor - 1) = self.get_c(self.cursor - 1).get_v128(t)
    }

    pub fn clear_rc_garbage(&mut self) {
        let nil = GosValue::new_nil();
        for i in self.cursor..self.max {
            self.rc[i] = nil.clone();
        }
    }
}

/// store iterators for Opcode::RANGE
pub struct RangeStack {
    maps: Vec<GosHashMapIter<'static>>,
    slices: Vec<SliceEnumIter<'static>>,
    strings: Vec<StringEnumIter<'static>>,
}

impl RangeStack {
    pub fn new() -> RangeStack {
        RangeStack {
            maps: vec![],
            slices: vec![],
            strings: vec![],
        }
    }

    pub fn range_init(&mut self, target: &GosValue) {
        match target {
            GosValue::Map(m) => {
                let map = m.0.borrow_data();
                let iter = unsafe { mem::transmute(map.iter()) };
                self.maps.push(iter);
            }
            GosValue::Slice(sl) => {
                let slice = sl.0.borrow();
                let iter = unsafe { mem::transmute(slice.iter().enumerate()) };
                self.slices.push(iter);
            }
            GosValue::Str(s) => {
                let iter = unsafe { mem::transmute(s.iter().enumerate()) };
                self.strings.push(iter);
            }
            _ => unreachable!(),
        }
    }

    pub fn range_body(&mut self, typ: ValueType, stack: &mut Stack) -> bool {
        match typ {
            ValueType::Map => match self.maps.last_mut().unwrap().next() {
                Some((k, v)) => {
                    stack.push(k.clone());
                    stack.push(v.clone().into_inner());
                    false
                }
                None => {
                    self.maps.pop();
                    true
                }
            },
            ValueType::Slice => match self.slices.last_mut().unwrap().next() {
                Some((k, v)) => {
                    stack.push_int(k as isize);
                    stack.push(v.clone().into_inner());
                    false
                }
                None => {
                    self.slices.pop();
                    true
                }
            },
            ValueType::Str => match self.strings.last_mut().unwrap().next() {
                Some((k, v)) => {
                    stack.push_int(k as isize);
                    stack.push_int(v as isize);
                    false
                }
                None => {
                    self.strings.pop();
                    true
                }
            },
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_stack() {
        let mut s = Stack::new();
        s.push(GosValue::Int(1));
        //assert_eq!(s.pop(), GosValue::Int(1));

        s.push(GosValue::new_str("11".to_string()));
        let v2 = GosValue::new_str("aa".to_string());
        s.set(0, v2.clone());
        //assert_eq!(s.get(0, ValueType::Str), v2);
    }
}
