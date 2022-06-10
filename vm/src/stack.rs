// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::value::*;

const DEFAULT_CAPACITY: usize = 1024;

pub struct Stack {
    vec: Vec<GosValue>,
}

impl Stack {
    #[inline]
    pub fn new() -> Stack {
        Stack {
            vec: vec![GosValue::new_nil(ValueType::Void); DEFAULT_CAPACITY],
        }
    }

    #[inline]
    pub fn with_vec(v: Vec<GosValue>) -> Stack {
        let mut s = Stack { vec: v };
        s.set_min_size(DEFAULT_CAPACITY);
        s
    }

    #[inline]
    pub fn get(&self, index: OpIndex) -> &GosValue {
        unsafe { self.vec.get_unchecked(index as usize) }
    }

    #[inline]
    pub fn get_mut(&mut self, index: OpIndex) -> &mut GosValue {
        unsafe { self.vec.get_unchecked_mut(index as usize) }
    }

    #[inline]
    pub fn set(&mut self, index: OpIndex, val: GosValue) {
        *self.get_mut(index) = val;
    }

    #[inline]
    pub fn read<'a>(&'a self, i: OpIndex, sb: OpIndex, consts: &'a [GosValue]) -> &'a GosValue {
        if i >= 0 {
            &self.get(i + sb)
        } else {
            &consts[(-i - 1) as usize]
        }
    }

    #[inline]
    pub fn read_and_op(
        &self,
        lhs: &ValueData,
        t: ValueType,
        op: Opcode,
        rhs: OpIndex,
        sb: OpIndex,
        consts: &[GosValue],
    ) -> GosValue {
        let d = match op {
            Opcode::INC => lhs.inc(t),
            Opcode::DEC => lhs.dec(t),
            Opcode::ADD => lhs.binary_op_add(self.read(rhs, sb, consts).data(), t),
            Opcode::SUB => lhs.binary_op_sub(self.read(rhs, sb, consts).data(), t),
            Opcode::MUL => lhs.binary_op_mul(self.read(rhs, sb, consts).data(), t),
            Opcode::QUO => lhs.binary_op_quo(self.read(rhs, sb, consts).data(), t),
            Opcode::REM => lhs.binary_op_rem(self.read(rhs, sb, consts).data(), t),
            Opcode::AND => lhs.binary_op_and(self.read(rhs, sb, consts).data(), t),
            Opcode::OR => lhs.binary_op_or(self.read(rhs, sb, consts).data(), t),
            Opcode::XOR => lhs.binary_op_xor(self.read(rhs, sb, consts).data(), t),
            Opcode::AND_NOT => lhs.binary_op_and_not(self.read(rhs, sb, consts).data(), t),
            Opcode::SHL => lhs.binary_op_shl(self.read(rhs, sb, consts).data().as_uint32(), t),
            Opcode::SHR => lhs.binary_op_shr(self.read(rhs, sb, consts).data().as_uint32(), t),
            _ => unreachable!(),
        };
        GosValue::new(t, d)
    }

    #[inline]
    pub fn set_min_size(&mut self, size: usize) {
        if size > self.vec.len() {
            self.vec.resize(size, GosValue::new_nil(ValueType::Void))
        }
    }

    #[inline]
    pub fn set_vec(&mut self, index: OpIndex, mut vec: Vec<GosValue>) {
        let begin = index as usize;
        let new_len = begin + vec.len();
        self.set_min_size(new_len);
        self.vec[begin..new_len].swap_with_slice(&mut vec);
    }

    #[inline]
    pub fn move_vec(&mut self, begin: OpIndex, end: OpIndex) -> Vec<GosValue> {
        let b = begin as usize;
        let e = end as usize;
        let mut defaults = vec![GosValue::new_nil(ValueType::Void); e - b];
        self.vec[b..e].swap_with_slice(&mut defaults[..]);
        defaults
    }

    #[inline]
    pub fn get_bool(&mut self, index: OpIndex) -> bool {
        *self.get_data(index).as_bool()
    }

    #[inline]
    pub fn set_bool(&mut self, index: OpIndex, b: bool) {
        *self.get_data_mut(index) = ValueData::new_bool(b);
    }

    #[inline]
    pub fn get_int(&mut self, index: OpIndex) -> isize {
        *self.get_data(index).as_int()
    }

    #[inline]
    pub fn set_int(&mut self, index: OpIndex, i: isize) {
        *self.get_data_mut(index) = ValueData::new_int(i);
    }

    #[inline]
    pub(crate) fn get_data(&self, index: OpIndex) -> &ValueData {
        self.get(index).data()
    }

    #[inline]
    pub(crate) fn get_data_mut(&mut self, index: OpIndex) -> &mut ValueData {
        unsafe { self.get_mut(index).data_mut() }
    }
}

/// store iterators for Opcode::RANGE
pub struct RangeStack {
    maps: Vec<GosHashMapIter<'static>>,
    slices: Vec<SliceEnumIter<'static, AnyElem>>,
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

    /// range_init creates iters and transmute them to 'static, then save them on stacks.
    /// it's safe because they are held only during 'ranging', which can never be longer
    /// than their real lifetime
    ///
    /// But it's not rust-safe just go-safe. because the Ref is dropped inside the transmute.
    /// that means if you write to the container we are ranging, it'll not be stopped by
    /// the borrow checker. Which is not safe to Rust, but it's exactly what Go does.
    pub fn range_init(
        &mut self,
        target: &GosValue,
        typ: ValueType,
        t_elem: ValueType,
    ) -> RuntimeResult<()> {
        match typ {
            ValueType::Map => {
                let map = target.as_some_map()?.0.borrow_data();
                let iter = unsafe { std::mem::transmute(map.iter()) };
                self.maps.push(iter);
            }
            ValueType::Array | ValueType::Slice => {
                let iter = dispatcher_a_s_for(t_elem).array_slice_iter(&target)?;
                self.slices.push(iter);
            }
            ValueType::String => {
                let iter = unsafe {
                    std::mem::transmute(StrUtil::as_str(target.as_string()).chars().enumerate())
                };
                self.strings.push(iter);
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    pub fn range_body(
        &mut self,
        typ: ValueType,
        t_elem: ValueType,
        stack: &mut Stack,
        index: OpIndex,
    ) -> bool {
        match typ {
            ValueType::Map => match self.maps.last_mut().unwrap().next() {
                Some((k, v)) => {
                    stack.set(index, k.clone());
                    stack.set(index + 1, v.clone());
                    false
                }
                None => {
                    self.maps.pop();
                    true
                }
            },
            ValueType::Array | ValueType::Slice => {
                match dispatcher_a_s_for(t_elem).array_slice_next(self.slices.last_mut().unwrap()) {
                    Some((k, v)) => {
                        stack.set_int(index, k as isize);
                        stack.set(index + 1, v);
                        false
                    }
                    None => {
                        self.slices.pop();
                        true
                    }
                }
            }
            ValueType::String => match self.strings.last_mut().unwrap().next() {
                Some((k, v)) => {
                    stack.set_int(index, k as isize);
                    stack.set_int(index + 1, v as isize);
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
