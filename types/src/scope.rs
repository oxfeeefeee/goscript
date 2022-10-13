// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.
//
//
// This code is adapted from the offical Go code written in Go
// with license as follows:
// Copyright 2013 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(dead_code)]
use super::objects::{ObjKey, ScopeKey, TCObjects};
use goscript_parser::{Map, Pos};
use std::fmt;

/// A Scope maintains a set of objects and links to its containing
/// (parent) and contained (children) scopes. Objects may be inserted
/// and looked up by name.
pub struct Scope {
    parent: Option<ScopeKey>,
    children: Vec<ScopeKey>,
    elems: Map<String, ObjKey>,
    pos: Pos, // scope pos; may be invalid
    end: Pos,
    comment: String, // for debugging only
    is_func: bool,   // set if this is a function scope (internal use only)
}

impl Scope {
    pub fn new(
        parent: Option<ScopeKey>,
        pos: Pos,
        end: Pos,
        comment: String,
        is_func: bool,
    ) -> Scope {
        Scope {
            parent: parent,
            children: Vec::new(),
            elems: Map::new(),
            pos: pos,
            end: end,
            comment: comment,
            is_func: is_func,
        }
    }

    pub fn add_child(&mut self, child: ScopeKey) {
        self.children.push(child)
    }

    pub fn parent(&self) -> &Option<ScopeKey> {
        &self.parent
    }

    pub fn children(&self) -> &Vec<ScopeKey> {
        &self.children
    }

    pub fn elems(&self) -> &Map<String, ObjKey> {
        &self.elems
    }

    /// len returns the number of scope elements.
    pub fn len(&self) -> usize {
        self.elems.len()
    }

    pub fn pos(&self) -> Pos {
        self.pos
    }

    pub fn set_pos(&mut self, p: Pos) {
        self.pos = p;
    }

    pub fn end(&self) -> Pos {
        self.end
    }

    pub fn set_end(&mut self, e: Pos) {
        self.end = e;
    }

    pub fn is_func(&self) -> bool {
        self.is_func
    }

    pub fn contains(&self, pos: Pos) -> bool {
        self.pos <= pos && pos <= self.end
    }

    /// name returns the scope's element names in sorted order.
    pub fn name(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.elems.keys().map(|x| x.clone()).collect();
        keys.sort();
        keys
    }

    pub fn innermost(&self, _pos: Pos) -> Option<ScopeKey> {
        unimplemented!() // used by Eval() which is not used?
    }

    pub fn lookup(&self, name: &str) -> Option<&ObjKey> {
        self.elems.get(name)
    }

    /// lookup_parent follows the parent chain of scopes starting with self until
    /// it finds a scope where lookup(name) returns a is_some() object, and then
    /// returns that scope and object. If a position pos is provided,
    /// only objects that were declared at or before pos are considered.
    /// If no such scope and object exists, it returns None.
    ///
    /// Note that obj.parent() may be different from the returned scope if the
    /// object was inserted into the scope and already had a parent at that
    /// time (see insert, below). This can only happen for dot-imported objects
    /// whose scope is the scope of the package that exported them.
    pub fn lookup_parent(
        self_key: &ScopeKey,
        name: &str,
        pos: Option<Pos>,
        objs: &TCObjects,
    ) -> Option<(ScopeKey, ObjKey)> {
        let mut scope_key = *self_key;
        let mut s = &objs.scopes[*self_key];
        loop {
            if let Some(okey) = s.lookup(name) {
                if pos.is_none() || *(objs.lobjs[*okey].scope_pos()) <= s.pos {
                    return Some((scope_key, *okey));
                }
            }
            if let Some(skey) = s.parent {
                scope_key = skey;
                s = &objs.scopes[skey];
            } else {
                break;
            }
        }
        None
    }

    /// insert attempts to insert an object obj into scope s.
    /// If s already contains an alternative object alt with
    /// the same name, insert leaves s unchanged and returns alt.
    /// Otherwise it inserts obj, sets the object's parent scope
    /// if not already set, and returns None.
    pub fn insert(self_key: ScopeKey, okey: ObjKey, objs: &mut TCObjects) -> Option<ObjKey> {
        let scope = &mut objs.scopes[self_key];
        let lang_obj = &mut objs.lobjs[okey];
        if let Some(obj) = scope.lookup(lang_obj.name()) {
            return Some(*obj);
        }
        scope.elems.insert(lang_obj.name().clone(), okey);
        if lang_obj.parent().is_none() {
            lang_obj.set_parent(Some(self_key));
        }
        None
    }

    /// fmt formats a string representation for the scope.
    /// with the scope elements sorted by name.
    /// The level of indentation is controlled by n >= 0, with
    /// n == 0 for no indentation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>, n: usize) -> fmt::Result {
        let ind = ".  ";
        let indn = ind.repeat(n);
        write!(f, "{}{} scope \n", indn, self.comment)?;
        let mut indn1 = indn.clone();
        indn1.push_str(ind);
        for name in self.elems.keys() {
            write!(f, "{}{}\n", &indn1, name)?;
        }
        Ok(())
    }
}

// ----------------------------------------------------------------------------
// utilities

/// fmt_scope_full formats the scope including it's children.
pub fn fmt_scope_full(
    skey: &ScopeKey,
    f: &mut fmt::Formatter<'_>,
    n: usize,
    objs: &TCObjects,
) -> fmt::Result {
    let ind = ".  ";
    let indn = ind.repeat(n);
    let sval = &objs.scopes[*skey];
    write!(f, "{}{} scope {:?}\n", indn, sval.comment, skey)?;
    sval.fmt(f, n)?;
    for child in sval.children.iter() {
        fmt_scope_full(child, f, n + 1, objs)?;
    }
    Ok(())
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt(f, 0)
    }
}
