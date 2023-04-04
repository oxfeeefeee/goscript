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
use crate::SourceRead;

use super::super::objects::{DeclInfoKey, ObjKey, TCObjects};
use super::check::{Checker, Initializer};
use super::resolver::DeclInfo;
use go_parser::Map;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Debug)]
struct GraphEdges {
    pred: Rc<RefCell<HashSet<ObjKey>>>,
    succ: Rc<RefCell<HashSet<ObjKey>>>,
}

struct GraphNode {
    obj: ObjKey,
    ndeps: usize,
    pos: usize,
}

impl GraphEdges {
    fn new(succ: Rc<RefCell<HashSet<ObjKey>>>) -> GraphEdges {
        GraphEdges {
            pred: Rc::new(RefCell::new(HashSet::new())),
            succ: succ,
        }
    }
}

impl<'a, S: SourceRead> Checker<'a, S> {
    pub fn init_order(&mut self) {
        let (mut nodes, edges) = self.dependency_graph();
        nodes.sort_by(|a, b| a.ndeps.cmp(&b.ndeps)); // sort by n_deps
        let len = nodes.len();
        let mut nodes = &mut nodes[0..len];
        let mut order: Vec<ObjKey> = vec![];
        let mut emitted: HashSet<DeclInfoKey> = HashSet::new();

        loop {
            if nodes.len() == 0 {
                break;
            }
            let mut first_dependant = nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.ndeps > 0)
                .map_or(nodes.len(), |(i, _)| i);
            if first_dependant == 0 {
                // we have a cycle with the first node
                let visited = &mut HashSet::new();
                let obj = nodes[0].obj;
                // If obj is not part of the cycle (e.g., obj->b->c->d->c),
                // cycle will be nil. Don't report anything in that case since
                // the cycle is reported when the algorithm gets to an object
                // in the cycle.
                // Furthermore, once an object in the cycle is encountered,
                // the cycle will be broken (dependency count will be reduced
                // below), and so the remaining nodes in the cycle don't trigger
                // another error (unless they are part of multiple cycles).
                if let Some(cycle) = find_path(&self.obj_map, obj, obj, visited, self.tc_objs) {
                    self.report_cycle(&cycle);
                }
                // Ok to continue, but the variable initialization order
                // will be incorrect at this point since it assumes no
                // cycle errors.
                // set first_dependant to 1 to remove the first node,
                first_dependant = 1;
            }

            let mut indep: Vec<ObjKey> = nodes[0..first_dependant].iter().map(|n| n.obj).collect();
            indep.sort_by(|a, b| self.lobj(*a).order().cmp(&self.lobj(*b).order()));
            order.append(&mut indep);

            // reduce dependency count of all dependent nodes
            let to_sub: Map<ObjKey, usize> =
                nodes[0..first_dependant]
                    .iter()
                    .fold(Map::new(), |mut init, x| {
                        for p in edges[&x.obj].pred.borrow().iter() {
                            *init.entry(*p).or_insert(0) += 1;
                        }
                        init
                    });
            // remove resolved nodes
            nodes = &mut nodes[first_dependant..];
            for n in nodes.iter_mut() {
                n.ndeps -= *to_sub.get(&n.obj).unwrap_or(&0);
            }
            // sort nodes, shoud be fast as it's almost sorted
            nodes.sort_by(|a, b| a.ndeps.cmp(&b.ndeps));
        }

        // record the init order for variables with initializers only
        let init_order = order
            .into_iter()
            .filter_map(|x| {
                let decl_key = self.obj_map[&x];
                match &self.tc_objs.decls[decl_key] {
                    DeclInfo::Var(var) => {
                        if var.init.is_none() {
                            return None;
                        }
                        // n:1 variable declarations such as: a, b = f()
                        // introduce a node for each lhs variable (here: a, b);
                        // but they all have the same initializer - emit only
                        // one, for the first variable seen
                        if emitted.contains(&decl_key) {
                            return None;
                        }
                        emitted.insert(decl_key);
                        let lhs = var.lhs.clone().unwrap_or(vec![x]);
                        Some(Initializer {
                            lhs: lhs,
                            rhs: var.init.clone().unwrap(),
                        })
                    }
                    _ => None,
                }
            })
            .collect();
        self.result.record_init_order(init_order);
    }

    /// dependency_graph returns the object dependency graph from the given obj_map,
    /// with any function nodes removed. The resulting graph contains only constants
    /// and variables.
    fn dependency_graph(&self) -> (Vec<GraphNode>, Map<ObjKey, GraphEdges>) {
        // map is the dependency (Object) -> graphNode mapping
        let map: Map<ObjKey, GraphEdges> = self.obj_map.iter().fold(
            Map::new(),
            |mut init: Map<ObjKey, GraphEdges>, (&x, &decl_key)| {
                if self.lobj(x).entity_type().is_dependency() {
                    let decl = &self.tc_objs.decls[decl_key];
                    let deps: HashSet<ObjKey> = decl.deps().iter().map(|z| *z).collect();
                    init.insert(x, GraphEdges::new(Rc::new(RefCell::new(deps))));
                }
                init
            },
        );

        // add the edges for the other direction
        for (o, node) in map.iter() {
            for s in node.succ.borrow().iter() {
                map[s].pred.borrow_mut().insert(*o);
            }
        }

        // remove function nodes and collect remaining graph nodes in graph
        // (Mutually recursive functions may introduce cycles among themselves
        // which are permitted. Yet such cycles may incorrectly inflate the dependency
        // count for variables which in turn may not get scheduled for initialization
        // in correct order.)
        let mut nodes: Vec<GraphNode> = map
            .iter()
            .filter_map(|(o, node)| {
                if self.lobj(*o).entity_type().is_func() {
                    for p in node.pred.borrow().iter() {
                        if p != o {
                            for s in node.succ.borrow().iter() {
                                if s != o {
                                    map[p].succ.borrow_mut().insert(*s);
                                    map[s].pred.borrow_mut().insert(*p);
                                    map[s].pred.borrow_mut().remove(o);
                                }
                            }
                            map[p].succ.borrow_mut().remove(o);
                        }
                    }
                    None
                } else {
                    Some(GraphNode {
                        obj: *o,
                        ndeps: node.succ.borrow().len(),
                        pos: self.lobj(*o).pos(),
                    })
                }
            })
            .collect();

        nodes.sort_by(|a, b| a.pos.cmp(&b.pos)); // sort by pos
        (nodes, map)
    }

    fn report_cycle(&self, cycle: &Vec<ObjKey>) {
        let o = self.lobj(cycle[0]);
        self.error(o.pos(), format!("initialization cycle for {}", o.name()));
        self.error(o.pos(), format!("\t{} refers to", o.name()));
        for okey in cycle[1..].iter().rev() {
            let o = self.lobj(*okey);
            self.error(o.pos(), format!("\t{} refers to", o.name()));
        }
        let o = self.lobj(cycle[0]);
        self.error(o.pos(), format!("\t{}", o.name()));
    }
}

/// find_path returns the (reversed) list of objects Vec<ObjKey>{to, ... from}
/// such that there is a path of object dependencies from 'from' to 'to'.
/// If there is no such path, the result is None.
fn find_path(
    map: &Map<ObjKey, DeclInfoKey>,
    from: ObjKey,
    to: ObjKey,
    visited: &mut HashSet<ObjKey>,
    tc_objs: &TCObjects,
) -> Option<Vec<ObjKey>> {
    if visited.contains(&from) {
        return None;
    }
    visited.insert(from);

    let decl = &tc_objs.decls[map[&from]];
    for &d in decl.deps().iter() {
        if d == to {
            return Some(vec![d]);
        }
        if let Some(mut p) = find_path(map, d, to, visited, tc_objs) {
            p.push(d);
            return Some(p);
        }
    }
    None
}
