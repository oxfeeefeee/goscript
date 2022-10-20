// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::gc::GcContainer;
use crate::value::*;
use std::hash::{Hash, Hasher};

/// Dispatcher is used to diapatch Array/Slice calls using the vtable.
pub(crate) trait Dispatcher {
    fn typ(&self) -> ValueType;

    fn array_with_size(
        &self,
        size: usize,
        cap: usize,
        val: &GosValue,
        gcos: &GcContainer,
    ) -> GosValue;

    fn array_with_data(&self, data: Vec<GosValue>, gcc: &GcContainer) -> GosValue;

    fn array_copy_semantic(&self, vdata: &ValueData, gcc: &GcContainer) -> ValueData;

    fn slice_copy_semantic(&self, vdata: &ValueData) -> ValueData;

    // you cannot just dispatch the default fn hash, as it makes this trait not object-safe
    fn array_hash(&self, val: &GosValue, state: &mut dyn Hasher);

    fn array_eq(&self, a: &ValueData, b: &ValueData) -> bool;

    fn array_cmp(&self, a: &ValueData, b: &ValueData) -> std::cmp::Ordering;

    fn array_get_vec(&self, val: &GosValue) -> Vec<GosValue>;

    fn slice_get_vec(&self, val: &GosValue) -> Option<Vec<GosValue>>;

    fn array_len(&self, val: &GosValue) -> usize;

    fn slice_slice(
        &self,
        slice: &GosValue,
        begin: isize,
        end: isize,
        max: isize,
    ) -> RuntimeResult<GosValue>;

    fn slice_array(&self, arr: GosValue, begin: isize, end: isize) -> RuntimeResult<GosValue>;

    fn slice_append(
        &self,
        this: GosValue,
        other: GosValue,
        gcc: &GcContainer,
    ) -> RuntimeResult<GosValue>;

    fn slice_copy_from(&self, this: GosValue, other: GosValue) -> usize;

    fn array_get(&self, from: &GosValue, i: usize) -> RuntimeResult<GosValue>;

    fn array_set(&self, to: &GosValue, val: &GosValue, i: usize) -> RuntimeResult<()>;

    fn slice_get(&self, from: &GosValue, i: usize) -> RuntimeResult<GosValue>;

    fn slice_set(&self, to: &GosValue, val: &GosValue, i: usize) -> RuntimeResult<()>;

    fn array_slice_iter(&self, val: &GosValue) -> RuntimeResult<SliceEnumIter<'static, AnyElem>>;

    fn array_slice_next(
        &self,
        iter: &mut SliceEnumIter<'static, AnyElem>,
    ) -> Option<(usize, GosValue)>;

    fn slice_swap(&self, slice: &GosValue, i: usize, j: usize) -> RuntimeResult<()>;
}

/// https://users.rust-lang.org/t/workaround-for-hash-trait-not-being-object-safe/53332/5
pub(crate) trait DynHash {
    fn dyn_hash(&self, state: &mut dyn Hasher);
}

impl<H: Hash + ?Sized> DynHash for H {
    fn dyn_hash(&self, mut state: &mut dyn Hasher) {
        self.hash(&mut state);
    }
}

macro_rules! define_dispatcher {
    ($dispatcher:tt, $elem:ty) => {
        pub(crate) struct $dispatcher {
            typ: ValueType,
        }

        impl $dispatcher {
            pub(crate) fn new(t: ValueType) -> $dispatcher {
                $dispatcher { typ: t }
            }
        }

        impl Dispatcher for $dispatcher {
            fn typ(&self) -> ValueType {
                self.typ
            }

            fn array_with_size(
                &self,
                size: usize,
                cap: usize,
                val: &GosValue,
                gcc: &GcContainer,
            ) -> GosValue {
                GosValue::new_array(
                    ArrayObj::<$elem>::with_size(size, cap, val, gcc),
                    self.typ,
                    gcc,
                )
            }

            #[inline]
            fn array_with_data(&self, data: Vec<GosValue>, gcc: &GcContainer) -> GosValue {
                GosValue::new_array(ArrayObj::<$elem>::with_data(data), self.typ, gcc)
            }

            #[inline]
            fn array_copy_semantic(&self, vdata: &ValueData, gcc: &GcContainer) -> ValueData {
                ValueData::new_array::<$elem>(vdata.as_array::<$elem>().0.clone(), gcc)
            }

            #[inline]
            fn slice_copy_semantic(&self, vdata: &ValueData) -> ValueData {
                match vdata.as_slice::<$elem>() {
                    Some(s) => ValueData::new_slice(s.0.clone()),
                    None => ValueData::new_nil(ValueType::Slice),
                }
            }

            #[inline]
            fn array_hash(&self, val: &GosValue, state: &mut dyn Hasher) {
                val.as_array::<$elem>().0.dyn_hash(state);
            }

            #[inline]
            fn array_eq(&self, a: &ValueData, b: &ValueData) -> bool {
                a.as_array::<$elem>().0 == b.as_array::<$elem>().0
            }

            #[inline]
            fn array_cmp(&self, a: &ValueData, b: &ValueData) -> std::cmp::Ordering {
                a.as_array::<$elem>().0.cmp(&b.as_array::<$elem>().0)
            }

            fn array_get_vec(&self, val: &GosValue) -> Vec<GosValue> {
                val.as_array::<$elem>()
                    .0
                    .as_rust_slice()
                    .iter()
                    .map(|x| x.clone().into_value(val.t_elem()))
                    .collect()
            }

            fn slice_get_vec(&self, val: &GosValue) -> Option<Vec<GosValue>> {
                val.as_slice::<$elem>().map(|x| {
                    x.0.as_rust_slice()
                        .iter()
                        .map(|y| y.clone().into_value(val.t_elem()))
                        .collect()
                })
            }

            #[inline]
            fn array_len(&self, val: &GosValue) -> usize {
                val.as_array::<$elem>().0.len()
            }

            #[inline]
            fn slice_slice(
                &self,
                slice: &GosValue,
                begin: isize,
                end: isize,
                max: isize,
            ) -> RuntimeResult<GosValue> {
                Ok(GosValue::new_slice(
                    slice
                        .as_non_nil_slice::<$elem>()?
                        .0
                        .slice(begin, end, max)?,
                    slice.t_elem,
                ))
            }

            #[inline]
            fn slice_array(
                &self,
                arr: GosValue,
                begin: isize,
                end: isize,
            ) -> RuntimeResult<GosValue> {
                Ok(GosValue::new_slice::<$elem>(
                    SliceObj::with_array(arr, begin, end)?,
                    self.typ,
                ))
            }

            #[inline]
            fn slice_append(
                &self,
                this: GosValue,
                other: GosValue,
                gcc: &GcContainer,
            ) -> RuntimeResult<GosValue> {
                let a = this.as_slice::<$elem>();
                let b = other.as_slice::<$elem>();
                match b {
                    Some(y) => match a {
                        Some(x) => {
                            let mut to = x.0.clone();
                            to.append(&y.0);
                            Ok(GosValue::new_slice(to, other.t_elem()))
                        }
                        None => {
                            let data = y.0.as_rust_slice().to_vec();
                            let arr = ArrayObj::<$elem>::with_raw_data(data);
                            let slice = SliceObj::<$elem>::with_array(
                                GosValue::new_array(arr, other.t_elem(), gcc),
                                0,
                                -1,
                            )?;
                            Ok(GosValue::new_slice(slice, other.t_elem()))
                        }
                    },
                    None => Ok(this),
                }
            }

            #[inline]
            fn slice_copy_from(&self, this: GosValue, other: GosValue) -> usize {
                let a = this.as_slice::<$elem>();
                let b = other.as_slice::<$elem>();
                match (a, b) {
                    (Some(x), Some(y)) => x.0.copy_from(&y.0),
                    _ => 0,
                }
            }

            #[inline]
            fn array_get(&self, from: &GosValue, i: usize) -> RuntimeResult<GosValue> {
                from.as_array::<$elem>().0.get(i, self.typ)
            }

            #[inline]
            fn array_set(&self, to: &GosValue, val: &GosValue, i: usize) -> RuntimeResult<()> {
                to.as_array::<$elem>().0.set(i, val)
            }

            #[inline]
            fn slice_get(&self, from: &GosValue, i: usize) -> RuntimeResult<GosValue> {
                from.as_non_nil_slice::<$elem>()?.0.get(i, self.typ)
            }

            #[inline]
            fn slice_set(&self, to: &GosValue, val: &GosValue, i: usize) -> RuntimeResult<()> {
                to.as_non_nil_slice::<$elem>()?.0.set(i, val)
            }

            #[inline]
            fn array_slice_iter(
                &self,
                val: &GosValue,
            ) -> RuntimeResult<SliceEnumIter<'static, AnyElem>> {
                let rust_slice = match val.typ() {
                    ValueType::Slice => val.as_non_nil_slice::<$elem>()?.0.as_rust_slice(),
                    ValueType::Array => val.as_array::<$elem>().0.as_rust_slice(),
                    _ => unreachable!(),
                };
                Ok(unsafe { std::mem::transmute(rust_slice.iter().enumerate()) })
            }

            #[inline]
            fn array_slice_next(
                &self,
                iter: &mut SliceEnumIter<'static, AnyElem>,
            ) -> Option<(usize, GosValue)> {
                let iter: &mut SliceEnumIter<'static, $elem> = unsafe { std::mem::transmute(iter) };
                match iter.next() {
                    Some((i, v)) => Some((i, v.clone().into_value(self.typ))),
                    None => None,
                }
            }

            #[inline]
            fn slice_swap(&self, slice: &GosValue, i: usize, j: usize) -> RuntimeResult<()> {
                slice.as_non_nil_slice::<$elem>()?.0.swap(i, j)
            }
        }
    };
}

#[derive(Clone, Copy)]
pub(crate) enum ElemType {
    ElemType8,
    ElemType16,
    ElemType32,
    ElemType64,
    ElemTypeGos,
}

pub(crate) struct ArrCaller {
    disp_array: [Box<dyn Dispatcher>; ValueType::Channel as usize + 1],
}

impl ArrCaller {
    pub fn new() -> ArrCaller {
        ArrCaller {
            disp_array: [
                Box::new(DispatcherGos::new(ValueType::Void)),
                Box::new(Dispatcher8::new(ValueType::Bool)),
                Box::new(Dispatcher64::new(ValueType::Int)),
                Box::new(Dispatcher8::new(ValueType::Int8)),
                Box::new(Dispatcher16::new(ValueType::Int16)),
                Box::new(Dispatcher32::new(ValueType::Int32)),
                Box::new(Dispatcher64::new(ValueType::Int64)),
                Box::new(Dispatcher64::new(ValueType::Uint)),
                Box::new(Dispatcher64::new(ValueType::UintPtr)),
                Box::new(Dispatcher8::new(ValueType::Uint8)),
                Box::new(Dispatcher16::new(ValueType::Uint16)),
                Box::new(Dispatcher32::new(ValueType::Uint32)),
                Box::new(Dispatcher64::new(ValueType::Uint64)),
                Box::new(Dispatcher32::new(ValueType::Float32)),
                Box::new(Dispatcher64::new(ValueType::Float64)),
                Box::new(Dispatcher64::new(ValueType::Complex64)),
                Box::new(Dispatcher64::new(ValueType::Function)),
                Box::new(Dispatcher64::new(ValueType::Package)),
                Box::new(DispatcherGos::new(ValueType::Metadata)),
                Box::new(DispatcherGos::new(ValueType::Complex128)),
                Box::new(DispatcherGos::new(ValueType::String)),
                Box::new(DispatcherGos::new(ValueType::Array)),
                Box::new(DispatcherGos::new(ValueType::Struct)),
                Box::new(DispatcherGos::new(ValueType::Pointer)),
                Box::new(DispatcherGos::new(ValueType::UnsafePtr)),
                Box::new(DispatcherGos::new(ValueType::Closure)),
                Box::new(DispatcherGos::new(ValueType::Slice)),
                Box::new(DispatcherGos::new(ValueType::Map)),
                Box::new(DispatcherGos::new(ValueType::Interface)),
                Box::new(DispatcherGos::new(ValueType::Channel)),
            ],
        }
    }

    #[inline]
    pub fn get(&self, t: ValueType) -> &Box<dyn Dispatcher> {
        &self.disp_array[t as usize]
    }

    #[inline]
    pub fn get_slow(t: ValueType) -> Box<dyn Dispatcher> {
        match t {
            ValueType::Int8 | ValueType::Uint8 => Box::new(Dispatcher8::new(t)),
            ValueType::Int16 | ValueType::Uint16 => Box::new(Dispatcher16::new(t)),
            ValueType::Int32 | ValueType::Uint32 | ValueType::Float32 => {
                Box::new(Dispatcher32::new(t))
            }
            ValueType::Int
            | ValueType::Uint
            | ValueType::Int64
            | ValueType::Uint64
            | ValueType::UintPtr
            | ValueType::Float64
            | ValueType::Function
            | ValueType::Package
            | ValueType::Complex64 => Box::new(Dispatcher64::new(t)),
            _ => Box::new(DispatcherGos::new(t)),
        }
    }

    #[inline]
    pub fn get_elem_type(t: ValueType) -> ElemType {
        const ELEM_TYPES: [ElemType; ValueType::Channel as usize + 1] = [
            ElemType::ElemTypeGos,
            ElemType::ElemType8,
            ElemType::ElemType64,
            ElemType::ElemType8,
            ElemType::ElemType16,
            ElemType::ElemType32,
            ElemType::ElemType64,
            ElemType::ElemType64,
            ElemType::ElemType64,
            ElemType::ElemType8,
            ElemType::ElemType16,
            ElemType::ElemType32,
            ElemType::ElemType64,
            ElemType::ElemType32,
            ElemType::ElemType64,
            ElemType::ElemType64,
            ElemType::ElemType64,
            ElemType::ElemType64,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
            ElemType::ElemTypeGos,
        ];
        ELEM_TYPES[t as usize]
    }

    // #[inline]
    // pub fn get_static(t: ValueType) -> &'static Box<dyn Dispatcher> {
    //     static mut DISPATCHERS: Option<ArrCaller> = None;
    //     unsafe {
    //         match &DISPATCHERS {
    //             Some(d) => &d.disp_array[t as usize],
    //             None => {
    //                 DISPATCHERS = Some(ArrCaller::new());
    //                 &DISPATCHERS.as_ref().unwrap().disp_array[t as usize]
    //             }
    //         }
    //     }
    // }
}
