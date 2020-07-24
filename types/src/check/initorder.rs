#![allow(dead_code)]
use super::super::constant::Value;
use super::super::importer::{Config, ImportKey, Importer};
use super::super::objects::{DeclInfoKey, ObjKey, PackageKey, ScopeKey, TCObjects, TypeKey};
use super::super::operand::OperandMode;
use super::super::selection::Selection;
use super::interface::IfaceInfo;
use goscript_parser::ast;
use goscript_parser::ast::Node;
use goscript_parser::ast::{Expr, NodeId};
use goscript_parser::errors::{ErrorList, FilePosErrors};
use goscript_parser::objects::{IdentKey, Objects as AstObjects};
use goscript_parser::position::Pos;
use goscript_parser::FileSet;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

struct GraphNode {
    obj: ObjKey,
    pred: HashSet<ObjKey>,
    succ: HashSet<ObjKey>,
    ndeps: isize,
}

impl GraphNode {
    fn new(obj: ObjKey) -> GraphNode {
        GraphNode {
            obj: obj,
            pred: HashSet::new(),
            succ: HashSet::new(),
            ndeps: 0,
        }
    }
}

/// dependency_graph returns the object dependency graph from the given obj_map,
/// with any function nodes removed. The resulting graph contains only constants
/// and variables.
fn dependency_graph(
    obj_map: &HashMap<ObjKey, DeclInfoKey>,
    objs: &TCObjects,
) -> HashMap<ObjKey, GraphNode> {
    let m: HashMap<ObjKey, GraphNode> = obj_map.iter().fold(
        HashMap::new(),
        |mut init: HashMap<ObjKey, GraphNode>, (&x, _)| {
            if objs.lobjs[x].entity_type().is_dependency() {
                init.insert(x, GraphNode::new(x));
            }
            init
        },
    );

    unimplemented!();
}
