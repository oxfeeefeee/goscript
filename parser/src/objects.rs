// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::ast;
use super::scope;
use slotmap::{new_key_type, SlotMap};
// use std::marker::PhantomData;
// use std::ops::Index;

// pub trait PiggyVecKey {
//     fn as_usize(&self) -> usize;
// }

// /// A vec that you can only insert into
// pub struct PiggyVec<K, V>
// where
//     K: PiggyVecKey + From<usize>,
// {
//     vec: Vec<V>,
//     phantom: PhantomData<K>,
// }

// impl<K, V> PiggyVec<K, V>
// where
//     K: PiggyVecKey + From<usize>,
// {
//     #[inline]
//     pub fn insert(&mut self, v: V) -> K {
//         self.vec.push(v);
//         (self.vec.len() - 1).into()
//     }
// }

// impl<K, V> Index<K> for PiggyVec<K, V>
// where
//     K: PiggyVecKey + From<usize>,
// {
//     type Output = V;

//     #[inline]
//     fn index(&self, i: K) -> &Self::Output {
//         &self.vec[i.as_usize()]
//     }
// }

// macro_rules! piggy_key_type {
//     ( $(#[$outer:meta])* $vis:vis struct $name:ident; $($rest:tt)* ) => {
//         $(#[$outer])*
//         #[derive(Copy, Clone, Default,
//                  Eq, PartialEq, Ord, PartialOrd,
//                  Hash, Debug)]
//         #[repr(transparent)]
//         $vis struct $name(usize);

//         impl From<usize> for $name {
//             fn from(k: usize) -> Self {
//                 $name(k)
//             }
//         }

//         impl $crate::PiggyVecKey for $name {
//             fn as_usize(&self) -> usize {
//                 self.0
//             }
//         }

//         $crate::new_key_type!($($rest)*);
//     };

//     () => {}
// }

new_key_type! { pub struct LabeledStmtKey; }
new_key_type! { pub struct AssignStmtKey; }
new_key_type! { pub struct SpecKey; }
new_key_type! { pub struct FuncDeclKey; }
new_key_type! { pub struct FuncTypeKey; }
new_key_type! { pub struct IdentKey; }
new_key_type! { pub struct FieldKey; }
new_key_type! { pub struct EntityKey; }
new_key_type! { pub struct ScopeKey; }

pub type LabeledStmts = SlotMap<LabeledStmtKey, ast::LabeledStmt>;
pub type AssignStmts = SlotMap<AssignStmtKey, ast::AssignStmt>;
pub type Specs = SlotMap<SpecKey, ast::Spec>;
pub type FuncDecls = SlotMap<FuncDeclKey, ast::FuncDecl>;
pub type FuncTypes = SlotMap<FuncTypeKey, ast::FuncType>;
pub type Idents = SlotMap<IdentKey, ast::Ident>;
pub type Fields = SlotMap<FieldKey, ast::Field>;
pub type Entitys = SlotMap<EntityKey, scope::Entity>;
pub type Scopes = SlotMap<ScopeKey, scope::Scope>;

pub struct Objects {
    pub l_stmts: LabeledStmts,
    pub a_stmts: AssignStmts,
    pub specs: Specs,
    pub fdecls: FuncDecls,
    pub ftypes: FuncTypes,
    pub idents: Idents,
    pub fields: Fields,
    pub entities: Entitys,
    pub scopes: Scopes,
}

impl Objects {
    pub fn new() -> Objects {
        const CAP: usize = 16;
        Objects {
            l_stmts: SlotMap::with_capacity_and_key(CAP),
            a_stmts: SlotMap::with_capacity_and_key(CAP),
            specs: SlotMap::with_capacity_and_key(CAP),
            fdecls: SlotMap::with_capacity_and_key(CAP),
            ftypes: SlotMap::with_capacity_and_key(CAP),
            idents: SlotMap::with_capacity_and_key(CAP),
            fields: SlotMap::with_capacity_and_key(CAP),
            entities: SlotMap::with_capacity_and_key(CAP),
            scopes: SlotMap::with_capacity_and_key(CAP),
        }
    }
}
