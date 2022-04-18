use super::gc::GcoVec;
use super::instruction::{Instruction, OpIndex, Opcode, ValueType};
use super::metadata::Meta;
use super::value::*;
use std::cmp::Ordering;
use std::fmt::{self, Display};
use std::mem;
use std::ptr;
use std::rc::Rc;

const DEFAULT_CAPACITY: usize = 1024;

macro_rules! stack_binary_op {
    ($stack:ident, $op:tt, $t:ident) => {{
        let len = $stack.len();
        let a = $stack.get_data(len - 2);
        let b = $stack.get_data(len - 1);
        *$stack.get_data_mut(len - 2) = a.$op(b, $t);
        $stack.pop_discard_copyable();
    }};
}

macro_rules! stack_binary_op_shift {
    ($stack:ident, $op:tt, $t0:ident, $t1:ident) => {{
        let mut right = $stack.pop_value();
        right.cast_copyable($t1, ValueType::Uint32);
        $stack
            .get_data_mut($stack.len() - 1)
            .$op(right.as_uint32(), $t0);
    }};
}

macro_rules! stack_cmp_op {
    ($stack:ident, $op:tt, $t:ident) => {{
        let len = $stack.len();
        let a = $stack.get_data(len - 2);
        let b = $stack.get_data(len - 1);
        *$stack.get_data_mut(len - 2) = ValueData::new_bool(ValueData::$op(a, b, $t));
        $stack.pop_discard_copyable();
    }};
}

macro_rules! read_with_ops {
    ($op:expr, $lhs:expr, $rhs:expr, $shift_rhs:expr, $t:expr) => {{
        match $op {
            Opcode::UNARY_ADD => {
                let mut v = unsafe { $lhs.copy_non_ptr() };
                v.inc($t);
                v
            } // INC
            Opcode::UNARY_SUB => {
                let mut v = unsafe { $lhs.copy_non_ptr() };
                v.dec($t);
                v
            } // DEC
            Opcode::ADD => $lhs.binary_op_add($rhs, $t),
            Opcode::SUB => $lhs.binary_op_sub($rhs, $t),
            Opcode::MUL => $lhs.binary_op_mul($rhs, $t),
            Opcode::QUO => $lhs.binary_op_quo($rhs, $t),
            Opcode::REM => $lhs.binary_op_rem($rhs, $t),
            Opcode::AND => $lhs.binary_op_and($rhs, $t),
            Opcode::OR => $lhs.binary_op_or($rhs, $t),
            Opcode::XOR => $lhs.binary_op_xor($rhs, $t),
            Opcode::AND_NOT => $lhs.binary_op_and_not($rhs, $t),
            Opcode::SHL => {
                let mut v = unsafe { $lhs.copy_non_ptr() };
                v.binary_op_shl($shift_rhs.as_uint32(), $t);
                v
            }
            Opcode::SHR => {
                let mut v = unsafe { $lhs.copy_non_ptr() };
                v.binary_op_shr($shift_rhs.as_uint32(), $t);
                v
            }
            _ => unreachable!(),
        }
    }};
}

macro_rules! store_local_val {
    ($stack:ident, $to:expr, $s_index:ident, $r_index:ident, $t:ident, $gcos:ident) => {{
        if $r_index < 0 {
            let ri = Stack::offset($stack.len(), $r_index);
            $to.set($s_index, $stack.copy_semantic(ri, $gcos));
        } else {
            $to.set(
                $s_index,
                $stack
                    .read_with_ops($to.get_data($s_index), $r_index, $t)
                    .into_value($t),
            );
        }
    }};
}

pub struct Stack {
    vec: Vec<GosValue>,
}

impl Stack {
    #[inline]
    pub fn new() -> Stack {
        Stack {
            vec: Vec::with_capacity(DEFAULT_CAPACITY),
        }
    }

    #[inline]
    pub fn get(&self, index: usize) -> &GosValue {
        unsafe { self.vec.get_unchecked(index) }
    }

    #[inline]
    pub fn get_data(&self, index: usize) -> &ValueData {
        self.get(index).data()
    }

    #[inline]
    pub fn get_mut(&mut self, index: usize) -> &mut GosValue {
        unsafe { self.vec.get_unchecked_mut(index) }
    }

    #[inline]
    pub fn set(&mut self, index: usize, val: GosValue) {
        self.vec[index] = val;
    }

    #[inline]
    pub fn push(&mut self, val: GosValue) {
        self.vec.push(val);
    }

    #[inline]
    pub fn push_from_index(&mut self, index: usize) {
        self.push(self.get(index).clone());
    }

    #[inline]
    pub fn push_nil(&mut self, t: ValueType) {
        self.push(GosValue::new_nil(t));
    }

    #[inline]
    pub fn push_bool(&mut self, b: bool) {
        self.push(GosValue::new_bool(b));
    }

    #[inline]
    pub fn push_int(&mut self, i: isize) {
        self.push(GosValue::new_int(i));
    }

    #[inline]
    pub fn push_int32_as(&mut self, i: i32, t: ValueType) {
        self.push(GosValue::int32_as(i, t));
    }

    #[inline]
    pub fn append_vec(&mut self, mut vec: Vec<GosValue>) {
        for v in vec.drain(..) {
            self.push(v);
        }
    }

    #[inline]
    pub fn append(&mut self, mut other: Stack) {
        self.vec.append(&mut other.vec)
    }

    #[inline]
    pub fn pop_value(&mut self) -> GosValue {
        self.vec.pop().unwrap()
    }

    #[inline]
    pub fn pop_bool(&mut self) -> bool {
        *self.pop_value().as_bool()
    }

    #[inline]
    pub fn pop_int(&mut self) -> isize {
        *self.pop_value().as_int()
    }

    #[inline]
    pub fn pop_int32(&mut self) -> i32 {
        *self.pop_value().as_int32()
    }

    #[inline]
    pub fn pop_uint(&mut self) -> usize {
        *self.pop_value().as_uint()
    }

    #[inline]
    pub fn pop_uint32(&mut self) -> u32 {
        *self.pop_value().as_uint32()
    }

    #[inline]
    pub fn pop_float32(&mut self) -> F32 {
        *self.pop_value().as_float32()
    }

    #[inline]
    pub fn pop_float64(&mut self) -> F64 {
        *self.pop_value().as_float64()
    }

    #[inline]
    pub fn pop_complex64(&mut self) -> Complex64 {
        *self.pop_value().as_complex64()
    }

    #[inline]
    pub fn pop_metadata(&mut self) -> Box<Meta> {
        self.pop_value().into_metadata()
    }

    #[inline]
    pub fn pop_string(&mut self) -> Rc<StringObj> {
        self.pop_value().into_string()
    }

    #[inline]
    pub fn pop_some_pointer(&mut self) -> RuntimeResult<Box<PointerObj>> {
        self.pop_value().into_some_pointer()
    }

    #[inline]
    pub fn pop_unsafe_ptr(&mut self) -> OptionBox<Rc<dyn UnsafePtr>> {
        self.pop_value().into_unsafe_ptr()
    }

    #[inline]
    pub fn pop_array<T>(&mut self) -> Rc<(ArrayObj<T>, RCount)> {
        self.pop_value().into_array()
    }

    #[inline]
    pub fn pop_slice<T>(&mut self) -> OptionRc<(SliceObj<T>, RCount)> {
        self.pop_value().into_slice()
    }

    #[inline]
    pub fn pop_some_slice<T>(&mut self) -> RuntimeResult<Rc<(SliceObj<T>, RCount)>> {
        self.pop_value().into_some_slice()
    }

    #[inline]
    pub fn pop_map(&mut self) -> OptionRc<(MapObj, RCount)> {
        self.pop_value().into_map()
    }

    #[inline]
    pub fn pop_channel(&mut self) -> OptionRc<ChannelObj> {
        self.pop_value().into_channel()
    }

    #[inline]
    pub fn pop_interface(&mut self) -> Option<Rc<InterfaceObj>> {
        self.pop_value().into_interface()
    }

    #[inline]
    pub fn pop_some_interface(&mut self) -> RuntimeResult<Rc<InterfaceObj>> {
        self.pop_value().into_some_interface()
    }

    #[inline]
    pub fn pop_closure(&mut self) -> OptionRc<(ClosureObj, RCount)> {
        self.pop_value().into_closure()
    }

    #[inline]
    pub fn pop_discard_copyable(&mut self) {
        self.pop_value();
    }

    #[inline]
    pub fn pop_value_n(&mut self, n: usize) -> Vec<GosValue> {
        self.vec.split_off(self.len() - n)
    }

    #[inline]
    pub fn discard_n(&mut self, n: usize) {
        self.pop_value_n(n);
    }

    #[inline]
    pub fn move_from(other: &mut Stack, n: usize) -> Stack {
        let values = other.pop_value_n(n);
        let mut stack = Stack::new();
        for v in values.into_iter() {
            stack.push(v);
        }
        stack
    }

    #[inline]
    pub fn split_off_with_type(&mut self, index: usize) -> Vec<GosValue> {
        self.vec.split_off(index).into_iter().collect()
    }

    #[inline]
    pub fn get_slice<T>(&mut self, index: usize) -> Option<&(SliceObj<T>, RCount)> {
        self.get(index).as_slice()
    }

    #[inline]
    pub fn get_string(&mut self, index: usize) -> &StringObj {
        self.get(index).as_string()
    }

    #[inline]
    pub fn copy_semantic(&self, index: usize, gcv: &GcoVec) -> GosValue {
        self.get(index).copy_semantic(gcv)
    }

    #[inline]
    fn read_with_ops(&self, lhs: &ValueData, r_index: OpIndex, t: ValueType) -> ValueData {
        let ri = Stack::offset(self.len(), -1);
        let op = Instruction::index2code(r_index);
        read_with_ops!(op, lhs, self.get_data(ri), self.get_data(ri), t)
    }

    #[inline]
    pub fn store_val(&self, target: &mut GosValue, r_index: OpIndex, t: ValueType, gcos: &GcoVec) {
        *target = if r_index < 0 {
            let i = Stack::offset(self.len(), r_index);
            self.copy_semantic(i, gcos)
        } else {
            self.read_with_ops(target.data(), r_index, t).into_value(t)
        };
    }

    #[inline]
    pub fn store_local(&mut self, s_index: usize, r_index: OpIndex, t: ValueType, gcos: &GcoVec) {
        store_local_val!(self, self, s_index, r_index, t, gcos);
    }

    #[inline]
    pub fn store_up_value(
        &mut self,
        upvalue: &UpValue,
        rhs_index: OpIndex,
        typ: ValueType,
        gcos: &GcoVec,
    ) {
        match &mut upvalue.inner.borrow_mut() as &mut UpValueState {
            UpValueState::Open(desc) => {
                let index = (desc.stack_base + desc.index) as usize;
                let uv_stack = desc.stack.upgrade().unwrap();
                match ptr::eq(uv_stack.as_ptr(), self) {
                    true => store_local_val!(self, self, index, rhs_index, typ, gcos),
                    false => store_local_val!(
                        self,
                        &mut uv_stack.borrow_mut(),
                        index,
                        rhs_index,
                        typ,
                        gcos
                    ),
                };
            }
            UpValueState::Closed(v) => {
                self.store_val(v, rhs_index, typ, gcos);
            }
        }
    }

    #[inline]
    pub fn store_index(
        &self,
        target: &GosValue,
        key: &GosValue,
        r_index: OpIndex,
        t: ValueType,
        gcv: &GcoVec,
    ) -> RuntimeResult<()> {
        match target.typ() {
            ValueType::Array => self.store_array_entry(target, *key.as_uint(), r_index, t, gcv),
            ValueType::Slice => self.store_slice_entry(target, *key.as_uint(), r_index, t, gcv),
            ValueType::Map => self.store_map_entry(target, key, r_index, t, gcv),
            _ => {
                unreachable!();
            }
        }
    }

    #[inline]
    pub fn store_index_int(
        &self,
        target: &GosValue,
        i: OpIndex,
        r_index: OpIndex,
        t: ValueType,
        gcv: &GcoVec,
    ) -> RuntimeResult<()> {
        match target.typ() {
            ValueType::Array => self.store_array_entry(target, i as usize, r_index, t, gcv),
            ValueType::Slice => self.store_slice_entry(target, i as usize, r_index, t, gcv),
            ValueType::Map => {
                self.store_map_entry(target, &GosValue::new_int(i as isize), r_index, t, gcv)
            }
            _ => {
                unreachable!();
            }
        }
    }

    #[inline]
    pub fn store_field(
        &self,
        target: &GosValue,
        key: &GosValue,
        r_index: OpIndex,
        t: ValueType,
        gcos: &GcoVec,
    ) {
        let target = &mut target.as_struct().0.borrow_fields_mut()[*key.as_int() as usize];
        self.store_val(target, r_index, t, gcos);
    }

    pub fn store_to_pointer(
        &mut self,
        p: &PointerObj,
        rhs_index: OpIndex,
        typ: ValueType,
        packages: &PackageObjs,
        gcv: &GcoVec,
    ) -> RuntimeResult<()> {
        match p {
            PointerObj::Default(obj) => {
                PointerObj::set_pointee_from(obj, self.get(Stack::offset(self.len(), rhs_index)));
            }
            PointerObj::UpVal(uv) => {
                self.store_up_value(uv, rhs_index, typ, gcv);
            }
            PointerObj::SliceMember(s, index) => {
                let index = *index as usize;
                let (array, index) = s.as_some_slice::<AnyElem>()?.0.get_array_equivalent(index);
                self.store_array_entry(array, index, rhs_index, typ, gcv)?;
            }
            PointerObj::StructField(s, index) => {
                let target: &mut GosValue =
                    &mut s.as_struct().0.borrow_fields_mut()[*index as usize];
                self.store_val(target, rhs_index, typ, gcv);
            }
            PointerObj::PkgMember(p, index) => {
                let target: &mut GosValue = &mut packages[*p].member_mut(*index);
                self.store_val(target, rhs_index, typ, gcv);
            }
        };
        Ok(())
    }

    #[inline]
    pub fn push_index_comma_ok(
        &mut self,
        map: &GosValue,
        index: &GosValue,
        gcv: &GcoVec,
    ) -> RuntimeResult<()> {
        let (v, b) = map.as_some_map()?.0.get(index, gcv);
        self.push(v);
        self.push_bool(b);
        Ok(())
    }

    #[inline]
    pub fn pack_variadic(&mut self, index: usize, t: ValueType, gcos: &GcoVec) {
        if index <= self.len() {
            let v = self.split_off_with_type(index);
            self.push(GosValue::slice_with_data(v, t, gcos))
        }
    }

    #[inline]
    pub fn add(&mut self, t: ValueType) {
        stack_binary_op!(self, binary_op_add, t)
    }

    #[inline]
    pub fn switch_cmp(&mut self, t: ValueType, objs: &VMObjects) -> bool {
        if t.copyable() {
            let len = self.len();
            let a = self.get_data(len - 2);
            let b = self.get_data(len - 1);
            let result = ValueData::compare_eql(a, b, t);
            self.pop_discard_copyable();
            result
        } else {
            let a = self.get(self.len() - 2);
            let b = self.get(self.len() - 1);
            let result = if t != ValueType::Metadata {
                a.eq(&b)
            } else {
                a.as_metadata().identical(b.as_metadata(), &objs.metas)
            };
            self.pop_value();
            result
        }
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
        stack_binary_op_shift!(self, binary_op_shl, t0, t1)
    }

    #[inline]
    pub fn shr(&mut self, t0: ValueType, t1: ValueType) {
        stack_binary_op_shift!(self, binary_op_shr, t0, t1)
    }

    #[inline]
    pub fn and_not(&mut self, t: ValueType) {
        stack_binary_op!(self, binary_op_and_not, t)
    }

    #[inline]
    pub fn unary_negate(&mut self, t: ValueType) {
        self.get_data_mut(self.len() - 1).unary_negate(t);
    }

    #[inline]
    pub fn unary_xor(&mut self, t: ValueType) {
        self.get_data_mut(self.len() - 1).unary_xor(t);
    }

    #[inline]
    pub fn logical_not(&mut self, t: ValueType) {
        self.get_data_mut(self.len() - 1).unary_not(t);
    }

    #[inline]
    pub fn compare_eql(&mut self, t0: ValueType, t1: ValueType) {
        if t0.copyable() && t0 == t1 {
            stack_cmp_op!(self, compare_eql, t0);
        } else {
            let (b, a) = (self.pop_value(), self.pop_value());
            self.push_bool(a.eq(&b));
        }
    }

    #[inline]
    pub fn compare_neq(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_neq, t);
        } else {
            let (b, a) = (self.pop_value(), self.pop_value());
            self.push_bool(!a.eq(&b));
        }
    }

    #[inline]
    pub fn compare_lss(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_lss, t);
        } else {
            let (b, a) = (self.pop_value(), self.pop_value());
            self.push_bool(a.cmp(&b) == Ordering::Less);
        }
    }

    #[inline]
    pub fn compare_gtr(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_gtr, t);
        } else {
            let (b, a) = (self.pop_value(), self.pop_value());
            self.push_bool(a.cmp(&b) == Ordering::Greater);
        }
    }

    #[inline]
    pub fn compare_leq(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_leq, t);
        } else {
            let (b, a) = (self.pop_value(), self.pop_value());
            self.push_bool(a.cmp(&b) != Ordering::Greater);
        }
    }

    #[inline]
    pub fn compare_geq(&mut self, t: ValueType) {
        if t.copyable() {
            stack_cmp_op!(self, compare_geq, t);
        } else {
            let (b, a) = (self.pop_value(), self.pop_value());
            self.push_bool(a.cmp(&b) != Ordering::Less);
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    #[inline]
    pub fn offset(base: usize, offset: OpIndex) -> usize {
        (base as isize + offset as isize) as usize
    }

    #[inline]
    fn get_data_mut(&mut self, index: usize) -> &mut ValueData {
        unsafe { self.get_mut(index).data_mut() }
    }

    /// helper function for store_index
    #[inline(always)]
    fn store_array_entry(
        &self,
        target: &GosValue,
        index: usize,
        r_index: OpIndex,
        t: ValueType,
        gcv: &GcoVec,
    ) -> RuntimeResult<()> {
        if r_index < 0 {
            let val = self
                .get(Stack::offset(self.len(), r_index))
                .copy_semantic(gcv);
            target.dispatcher_a_s().array_set(target, &val, index)?;
        } else {
            let val = target.dispatcher_a_s().array_get(target, index)?;
            let val = self.read_with_ops(val.data(), r_index, t).into_value(t);
            target.dispatcher_a_s().array_set(target, &val, index)?;
        }
        Ok(())
    }

    /// helper function for store_index
    #[inline(always)]
    fn store_slice_entry(
        &self,
        target: &GosValue,
        index: usize,
        r_index: OpIndex,
        t: ValueType,
        gcv: &GcoVec,
    ) -> RuntimeResult<()> {
        let (array, index) = target
            .as_some_slice::<AnyElem>()?
            .0
            .get_array_equivalent(index);
        self.store_array_entry(array, index, r_index, t, gcv)
    }

    /// helper function for store_index
    #[inline(always)]
    fn store_map_entry(
        &self,
        target: &GosValue,
        key: &GosValue,
        r_index: OpIndex,
        t: ValueType,
        gcv: &GcoVec,
    ) -> RuntimeResult<()> {
        let map = target.as_some_map()?;
        map.0.touch_key(&key, gcv);
        let mut borrowed = map.0.borrow_data_mut();
        let mut target_cell = borrowed.get_mut(&key).unwrap();
        self.store_val(&mut target_cell, r_index, t, gcv);
        Ok(())
    }
}

impl Display for Stack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl fmt::Debug for Stack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("=========top=======\n")?;
        for v in self.vec.iter().rev() {
            write!(f, "{:#?}\n", v)?;
        }
        f.write_str("=========botton====\n")
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
                let iter = unsafe { mem::transmute(map.iter()) };
                self.maps.push(iter);
            }
            ValueType::Array | ValueType::Slice => {
                let iter = dispatcher_a_s_for(t_elem).array_slice_iter(&target)?;
                self.slices.push(iter);
            }
            ValueType::String => {
                let iter = unsafe {
                    mem::transmute(StrUtil::as_str(target.as_string()).chars().enumerate())
                };
                self.strings.push(iter);
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    pub fn range_body(&mut self, typ: ValueType, t_elem: ValueType, stack: &mut Stack) -> bool {
        match typ {
            ValueType::Map => match self.maps.last_mut().unwrap().next() {
                Some((k, v)) => {
                    stack.push(k.clone());
                    stack.push(v.clone());
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
                        stack.push_int(k as isize);
                        stack.push(v);
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
