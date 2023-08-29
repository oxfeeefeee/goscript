// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use crate::value::*;
#[cfg(feature = "serde_borsh")]
use borsh::{maybestd::io::Result, maybestd::io::Write, BorshDeserialize, BorshSerialize};
#[cfg(feature = "serde_borsh")]
use go_parser::PiggyVecKey;
use go_parser::{piggy_key_type, PiggyVec};

#[cfg(feature = "serde_borsh")]
macro_rules! impl_borsh_for_key {
    ($key:ident) => {
        impl BorshSerialize for $key {
            fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
                self.as_usize().serialize(writer)
            }
        }

        impl BorshDeserialize for $key {
            fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> Result<Self> {
                let i: usize = usize::deserialize_reader(reader)?;
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

#[cfg(feature = "serde_borsh")]
impl_borsh_for_key!(MetadataKey);
#[cfg(feature = "serde_borsh")]
impl_borsh_for_key!(FunctionKey);
#[cfg(feature = "serde_borsh")]
impl_borsh_for_key!(PackageKey);

pub type MetadataObjs = PiggyVec<MetadataKey, MetadataType>;
pub type FunctionObjs = PiggyVec<FunctionKey, FunctionObj>;
pub type PackageObjs = PiggyVec<PackageKey, PackageObj>;

pub struct VMObjects {
    pub metas: MetadataObjs,
    pub functions: FunctionObjs,
    pub packages: PackageObjs,
    pub prim_meta: PrimitiveMeta,
    pub(crate) arr_slice_caller: Box<ArrCaller>,
}

impl VMObjects {
    pub fn new() -> VMObjects {
        const CAP: usize = 16;
        let mut metas = PiggyVec::with_capacity(CAP);
        let prim_meta = PrimitiveMeta::new(&mut metas);
        VMObjects {
            metas,
            functions: PiggyVec::with_capacity(CAP),
            packages: PiggyVec::with_capacity(CAP),
            prim_meta,
            arr_slice_caller: Box::new(ArrCaller::new()),
        }
    }
}

#[cfg(feature = "serde_borsh")]
impl BorshSerialize for VMObjects {
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
        self.metas.vec().serialize(writer)?;
        self.functions.vec().serialize(writer)?;
        self.packages.vec().serialize(writer)
    }
}

#[cfg(feature = "serde_borsh")]
impl BorshDeserialize for VMObjects {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> Result<Self> {
        let mut metas = Vec::<MetadataType>::deserialize_reader(reader)?.into();
        let functions = Vec::<FunctionObj>::deserialize_reader(reader)?.into();
        let packages = Vec::<PackageObj>::deserialize_reader(reader)?.into();
        let prim_meta = PrimitiveMeta::new(&mut metas);
        Ok(VMObjects {
            metas,
            functions,
            packages,
            prim_meta,
            arr_slice_caller: Box::new(ArrCaller::new()),
        })
    }
}

#[cfg_attr(feature = "serde_borsh", derive(BorshDeserialize, BorshSerialize))]
pub struct Bytecode {
    pub objects: VMObjects,
    pub consts: Vec<GosValue>,
    /// For calling method via interfaces
    pub ifaces: Vec<(Meta, Vec<Binding4Runtime>)>,
    /// For embedded fields of structs
    pub indices: Vec<Vec<OpIndex>>,
    pub entry: FunctionKey,
    pub main_pkg: PackageKey,
    /// Optional, for debug info
    pub file_set: Option<go_parser::FileSet>,
}

impl Bytecode {
    pub fn new(
        objects: VMObjects,
        consts: Vec<GosValue>,
        ifaces: Vec<(Meta, Vec<IfaceBinding>)>,
        indices: Vec<Vec<OpIndex>>,
        entry: FunctionKey,
        main_pkg: PackageKey,
        file_set: Option<go_parser::FileSet>,
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
            main_pkg,
            file_set,
        }
    }
}
