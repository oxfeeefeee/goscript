// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::value::*;
use borsh::{maybestd::io::Result, maybestd::io::Write, BorshDeserialize, BorshSerialize};
use goscript_parser::objects::{PiggyVec, PiggyVecKey};
use goscript_parser::piggy_key_type;

macro_rules! impl_borsh_for_key {
    ($key:ident) => {
        impl BorshSerialize for $key {
            fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
                self.as_usize().serialize(writer)
            }
        }

        impl BorshDeserialize for $key {
            fn deserialize(buf: &mut &[u8]) -> Result<Self> {
                let i: usize = usize::deserialize(buf)?;
                Ok(i.into())
            }
        }
    };
}

piggy_key_type! {
    pub struct MetadataKey;
    pub struct FunctionKey;
    pub struct PackageKey;
}

impl_borsh_for_key!(MetadataKey);
impl_borsh_for_key!(FunctionKey);
impl_borsh_for_key!(PackageKey);

pub type MetadataObjs = PiggyVec<MetadataKey, MetadataType>;
pub type FunctionObjs = PiggyVec<FunctionKey, FunctionObj>;
pub type PackageObjs = PiggyVec<PackageKey, PackageObj>;

pub struct VMObjects {
    pub metas: MetadataObjs,
    pub functions: FunctionObjs,
    pub packages: PackageObjs,
    pub s_meta: StaticMeta,
    pub(crate) arr_slice_caller: Box<ArrCaller>,
}

impl VMObjects {
    pub fn new() -> VMObjects {
        const CAP: usize = 16;
        let mut metas = PiggyVec::with_capacity(CAP);
        let s_meta = StaticMeta::new(&mut metas);
        VMObjects {
            metas,
            functions: PiggyVec::with_capacity(CAP),
            packages: PiggyVec::with_capacity(CAP),
            s_meta,
            arr_slice_caller: Box::new(ArrCaller::new()),
        }
    }
}

impl BorshSerialize for VMObjects {
    fn serialize<W: Write>(&self, _writer: &mut W) -> Result<()> {
        unimplemented!();
        // self.metas.vec().serialize(writer)?;
        // self.functions.vec().serialize(writer)?;
        // self.packages.vec().serialize(writer)
    }
}

impl BorshDeserialize for VMObjects {
    fn deserialize(_buf: &mut &[u8]) -> Result<Self> {
        unimplemented!();

        // let mut metas = Vec::<MetadataType>::deserialize(buf)?.into();
        // let functions = Vec::<FunctionObj>::deserialize(buf)?.into();
        // let packages = Vec::<PackageObj>::deserialize(buf)?.into();
        // let s_meta = StaticMeta::new(&mut metas);
        // Ok(VMObjects {
        //     metas,
        //     functions,
        //     packages,
        //     s_meta,
        //     arr_slice_caller: Box::new(ArrCaller::new()),
        // })
    }
}

pub struct Bytecode {
    pub objects: VMObjects,
    pub consts: Vec<GosValue>,
    /// For calling method via interfaces
    pub ifaces: Vec<(Meta, Vec<Binding4Runtime>)>,
    /// For embedded fields of structs
    pub indices: Vec<Vec<OpIndex>>,
    pub entry: FunctionKey,
}

impl Bytecode {
    pub fn new(
        objects: VMObjects,
        consts: Vec<GosValue>,
        ifaces: Vec<(Meta, Vec<IfaceBinding>)>,
        indices: Vec<Vec<OpIndex>>,
        entry: FunctionKey,
    ) -> Bytecode {
        let ifaces = ifaces
            .into_iter()
            .map(|(ms, binding)| {
                let binding = binding.into_iter().map(|x| x.into()).collect();
                (ms, binding)
            })
            .collect();
        Bytecode {
            objects,
            consts,
            ifaces,
            indices,
            entry,
        }
    }
}
