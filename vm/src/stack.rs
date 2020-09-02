#![allow(dead_code)]
use super::instruction::{Value32Type, COPYABLE_END};
use super::value::*;

pub struct Stack {
    inner: Vec<GosValue32>,
}

impl Stack {
    pub fn new() -> Stack {
        Stack { inner: Vec::new() }
    }

    #[inline]
    pub fn push(&mut self, val: GosValue) {
        let (v, _) = GosValue32::from_v64(val);
        self.inner.push(v);
    }

    #[inline]
    pub fn push_from_index(&mut self, index: usize, t: Value32Type) {
        self.inner
            .push(unsafe { self.inner.get_unchecked(index).clone(t) });
    }

    #[inline]
    pub fn push_nil(&mut self) {
        self.inner.push(GosValue32::nil());
    }

    #[inline]
    pub fn push_bool(&mut self, b: bool) {
        self.inner.push(GosValue32::from_bool(b));
    }

    #[inline]
    pub fn push_int(&mut self, i: isize) {
        self.inner.push(GosValue32::from_int(i));
    }

    #[inline]
    pub fn pop(&mut self) -> GosValue {
        let v32 = self.inner.pop().unwrap();
        v32.into_v64(v32.debug_type)
    }

    #[inline]
    pub fn pop_discard(&mut self, t: Value32Type) {
        if t <= COPYABLE_END {
            self.inner.pop();
        } else {
            self.inner.pop().unwrap().into_v64(t);
        }
    }

    #[inline]
    pub fn pop_non_rc(&mut self) {
        self.inner.pop();
    }

    #[inline]
    pub fn get(&self, index: usize /*, t: Value32Type*/) -> GosValue {
        let v = self.inner.get(index).unwrap();
        v.get_v64(v.debug_type)
    }

    #[inline]
    pub fn get2(&self, index: usize, t: Value32Type) -> GosValue {
        let v = self.inner.get(index).unwrap();
        v.get_v64(t)
    }

    #[inline]
    pub fn set(&mut self, index: usize, val: GosValue) {
        let (v, _) = GosValue32::from_v64(val);
        let _ = self.inner[index].into_v64(self.inner[index].debug_type);
        self.inner[index] = v;
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
        let mut v32 = v
            .drain(..)
            .map(|x| {
                let (v, _) = GosValue32::from_v64(x);
                v
            })
            .collect();
        self.inner.append(&mut v32);
    }

    #[inline]
    pub fn split_off(&mut self, index: usize) -> Vec<GosValue> {
        self.inner
            .split_off(index)
            .into_iter()
            .map(|x| x.into_v64(x.debug_type))
            .collect()
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
        //assert_eq!(s.get(0, Value32Type::Str), v2);
    }
}
