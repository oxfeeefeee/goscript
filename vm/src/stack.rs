#![allow(dead_code)]
use super::instruction::{Instruction, OpIndex, Opcode, ValueType, COPYABLE_END};
use super::value::*;

pub struct Stack {
    inner: Vec<GosValue64>,
}

impl Stack {
    pub fn new() -> Stack {
        Stack { inner: Vec::new() }
    }

    #[inline]
    pub fn push(&mut self, val: GosValue) {
        let (v, _) = GosValue64::from_v128_leak(&val);
        self.inner.push(v);
    }

    #[inline]
    pub fn push_from_index(&mut self, index: usize, t: ValueType) {
        self.inner.push(self.get_inner(index).clone(t));
    }

    #[inline]
    pub fn push_nil(&mut self) {
        self.inner.push(GosValue64::nil());
    }

    #[inline]
    pub fn push_bool(&mut self, b: bool) {
        self.inner.push(GosValue64::from_bool(b));
    }

    #[inline]
    pub fn push_int(&mut self, i: isize) {
        self.inner.push(GosValue64::from_int(i));
    }

    #[inline]
    pub fn pop(&mut self) -> GosValue {
        let v64 = self.inner.pop().unwrap();
        v64.into_v128_unleak(v64.debug_type)
    }

    #[inline]
    pub fn pop_discard(&mut self, t: ValueType) {
        if t <= COPYABLE_END {
            self.inner.pop();
        } else {
            self.inner.pop().unwrap().into_v128_unleak(t);
        }
    }

    #[inline]
    pub fn pop_with_type(&mut self, t: ValueType) -> GosValue {
        let v64 = self.inner.pop().unwrap();
        v64.into_v128_unleak(t)
    }

    #[inline]
    pub fn get(&self, index: usize /*, t: ValueType*/) -> GosValue {
        let v = self.inner.get(index).unwrap();
        v.get_v128(v.debug_type)
    }

    #[inline]
    pub fn get_with_type(&self, index: usize, t: ValueType) -> GosValue {
        let v = self.inner.get(index).unwrap();
        v.get_v128(t)
    }

    #[inline]
    pub fn set(&mut self, index: usize, val: GosValue) {
        let (v, _) = GosValue64::from_v128_leak(&val);
        let _ = self.inner[index].into_v128_unleak(self.inner[index].debug_type);
        self.inner[index] = v;
    }

    #[inline]
    pub fn set_with_type(&mut self, index: usize, val: GosValue64, t: ValueType) {
        let _ = self.inner[index].into_v128_unleak(t);
        self.inner[index] = val;
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn truncate(&mut self, len: usize) {
        self.split_off(len);
    }

    #[inline]
    pub fn append(&mut self, v: &mut Vec<GosValue>) {
        let mut v64 = v
            .drain(..)
            .map(|x| {
                let (v, _) = GosValue64::from_v128_leak(&x);
                v
            })
            .collect();
        self.inner.append(&mut v64);
    }

    #[inline]
    pub fn split_off(&mut self, index: usize) -> Vec<GosValue> {
        self.inner
            .split_off(index)
            .into_iter()
            .map(|x| x.into_v128_unleak(x.debug_type))
            .collect()
    }

    #[inline]
    pub fn store_copy_semantic(&mut self, li: usize, ri: usize, t: ValueType, zero: &ZeroVal) {
        //dbg!(t, self.inner[li].debug_type, self.inner[ri].debug_type);
        debug_assert!(t == self.inner[li].debug_type);
        debug_assert!(t == self.inner[ri].debug_type);
        if t <= COPYABLE_END {
            self.inner.swap(li, ri); // self.inner[li] = self.inner[ri];
        } else {
            self.set_with_type(li, self.inner[ri].copy_semantic(t, zero), t);
        }
    }

    #[inline]
    pub fn store_with_op(&mut self, li: usize, ri: usize, op: Opcode, t: ValueType) {
        let a = self.get_inner(li);
        let b = self.get_inner(ri);
        self.inner[li] = GosValue64::binary_op(*a, *b, t, op);
    }

    #[inline]
    pub fn store_val(&self, target: &mut GosValue, r_index: OpIndex, t: ValueType, zero: &ZeroVal) {
        let v64 = if r_index < 0 {
            let rhs_s_index = Stack::offset(self.len(), r_index);
            if t <= COPYABLE_END {
                *self.get_inner(rhs_s_index)
            } else {
                self.get_inner(rhs_s_index).copy_semantic(t, zero)
            }
        } else {
            let (a, at) = GosValue64::from_v128_leak(target);
            let ri = Stack::offset(self.len(), -1);
            let b = self.get_inner(ri);
            let op = Instruction::index2code(r_index);
            let v = GosValue64::binary_op(a, *b, t, op);
            a.into_v128_unleak(at);
            v
        };
        *target = v64.into_v128_unleak(t);
    }

    #[inline]
    fn get_inner(&self, i: usize) -> &GosValue64 {
        unsafe { self.inner.get_unchecked(i) }
    }

    #[inline]
    pub fn offset(base: usize, offset: OpIndex) -> usize {
        (base as isize + offset as isize) as usize
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
