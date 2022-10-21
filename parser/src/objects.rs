// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::ast;
use super::scope;
use std::marker::PhantomData;
use std::ops::Index;
use std::ops::IndexMut;

pub trait PiggyVecKey {
    fn as_usize(&self) -> usize;
}

/// A vec that you can only insert into, so that the index can be used as a key
///
#[derive(Debug)]
pub struct PiggyVec<K, V>
where
    K: PiggyVecKey + From<usize>,
{
    vec: Vec<V>,
    phantom: PhantomData<K>,
}

impl<K, V> PiggyVec<K, V>
where
    K: PiggyVecKey + From<usize>,
{
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            vec: Vec::with_capacity(capacity),
            phantom: PhantomData {},
        }
    }

    #[inline]
    pub fn insert(&mut self, v: V) -> K {
        self.vec.push(v);
        (self.vec.len() - 1).into()
    }

    #[inline]
    pub fn vec(&self) -> &Vec<V> {
        &self.vec
    }
}

impl<K, V> Index<K> for PiggyVec<K, V>
where
    K: PiggyVecKey + From<usize>,
{
    type Output = V;

    #[inline]
    fn index(&self, index: K) -> &Self::Output {
        &self.vec[index.as_usize()]
    }
}

impl<K, V> IndexMut<K> for PiggyVec<K, V>
where
    K: PiggyVecKey + From<usize>,
{
    #[inline]
    fn index_mut(&mut self, index: K) -> &mut Self::Output {
        &mut self.vec[index.as_usize()]
    }
}

impl<K, V> From<Vec<V>> for PiggyVec<K, V>
where
    K: PiggyVecKey + From<usize>,
{
    #[inline]
    fn from(vec: Vec<V>) -> Self {
        PiggyVec {
            vec,
            phantom: PhantomData {},
        }
    }
}

#[macro_export]
macro_rules! piggy_key_type {
    ( $(#[$outer:meta])* $vis:vis struct $name:ident; $($rest:tt)* ) => {
        $(#[$outer])*
        #[derive(Copy, Clone, Default,
                 Eq, PartialEq, Ord, PartialOrd,
                 Hash, Debug)]
        #[repr(transparent)]
        $vis struct $name(usize);

        impl $name {
            #[inline]
            pub fn null() -> Self {
                $name(std::usize::MAX)
            }
        }

        impl From<usize> for $name {
            #[inline]
            fn from(k: usize) -> Self {
                $name(k)
            }
        }

        impl $crate::PiggyVecKey for $name {
            #[inline]
            fn as_usize(&self) -> usize {
                self.0
            }
        }

        piggy_key_type!($($rest)*);
    };

    () => {}
}

piggy_key_type! {
    pub struct LabeledStmtKey;
    pub struct AssignStmtKey;
    pub struct SpecKey;
    pub struct FuncDeclKey;
    pub struct FuncTypeKey;
    pub struct IdentKey;
    pub struct FieldKey;
    pub struct EntityKey;
    pub struct ScopeKey;
}

pub type LabeledStmts = PiggyVec<LabeledStmtKey, ast::LabeledStmt>;
pub type AssignStmts = PiggyVec<AssignStmtKey, ast::AssignStmt>;
pub type Specs = PiggyVec<SpecKey, ast::Spec>;
pub type FuncDecls = PiggyVec<FuncDeclKey, ast::FuncDecl>;
pub type FuncTypes = PiggyVec<FuncTypeKey, ast::FuncType>;
pub type Idents = PiggyVec<IdentKey, ast::Ident>;
pub type Fields = PiggyVec<FieldKey, ast::Field>;
pub type Entitys = PiggyVec<EntityKey, scope::Entity>;
pub type Scopes = PiggyVec<ScopeKey, scope::Scope>;

pub struct AstObjects {
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

impl AstObjects {
    pub fn new() -> AstObjects {
        const CAP: usize = 16;
        AstObjects {
            l_stmts: PiggyVec::with_capacity(CAP),
            a_stmts: PiggyVec::with_capacity(CAP),
            specs: PiggyVec::with_capacity(CAP),
            fdecls: PiggyVec::with_capacity(CAP),
            ftypes: PiggyVec::with_capacity(CAP),
            idents: PiggyVec::with_capacity(CAP),
            fields: PiggyVec::with_capacity(CAP),
            entities: PiggyVec::with_capacity(CAP),
            scopes: PiggyVec::with_capacity(CAP),
        }
    }
}
