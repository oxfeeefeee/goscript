#![macro_use]
#![allow(unused_macros)]
use super::obj::LangObj;
use super::package::Package;
use super::scope::Scope;
use super::typ::Type;

use slotmap::{new_key_type, DenseSlotMap};

const DEFAULT_CAPACITY: usize = 16;

macro_rules! new_objects {
    () => {
        DenseSlotMap::with_capacity_and_key(DEFAULT_CAPACITY)
    };
}

new_key_type! { pub struct ObjKey; }
new_key_type! { pub struct TypeKey; }
new_key_type! { pub struct PackageKey; }
new_key_type! { pub struct ScopeKey; }

pub type LangObjs = DenseSlotMap<ObjKey, LangObj>;
pub type Types = DenseSlotMap<TypeKey, Type>;
pub type Packages = DenseSlotMap<PackageKey, Package>;
pub type Scopes = DenseSlotMap<ScopeKey, Scope>;

pub struct Objects {
    pub lobjs: LangObjs,
    pub types: Types,
    pub pkgs: Packages,
    pub scopes: Scopes,
}

impl Objects {
    pub fn new() -> Objects {
        Objects {
            lobjs: new_objects!(),
            types: new_objects!(),
            pkgs: new_objects!(),
            scopes: new_objects!(),
        }
    }
}
