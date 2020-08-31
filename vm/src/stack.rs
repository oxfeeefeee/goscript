#![allow(dead_code)]
use super::instruction::Value32Type;
use super::value::*;

#[derive(Clone)]
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
    pub fn pop(&mut self) -> GosValue {
        let v32 = self.inner.pop().unwrap();
        v32.into_v64(v32.debug_type)
    }

    #[inline]
    pub fn pop2(&mut self, t: Value32Type) {
        self.inner.pop().unwrap().into_v64(t);
    }

    #[inline]
    pub fn pop_non_rc(&mut self) {
        self.inner.pop();
    }

    #[inline]
    pub fn get(&self, index: usize) -> GosValue {
        let v = self.inner.get(index).unwrap();
        v.get_v64(v.debug_type)
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
        assert_eq!(s.pop(), GosValue::Int(1));

        s.push(GosValue::new_str("11".to_string()));
        let v2 = GosValue::new_str("aa".to_string());
        s.set(0, v2.clone());
        assert_eq!(s.get(0), v2);
    }
}
