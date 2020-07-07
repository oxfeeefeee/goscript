#![allow(dead_code)]
use super::check::MethodInfo;
use super::obj::fmt_obj;
use super::objects::{ObjKey, TCObjects, TypeKey};
use super::operand::{fmt_expr, fmt_expr_call, fmt_expr_interface, Operand};
use super::typ::fmt_type;
use goscript_parser::ast::Node;
use goscript_parser::ast::{CallExpr, Expr, InterfaceType};
use goscript_parser::objects::Objects as AstObjects;
use goscript_parser::Pos;
use std::fmt::{self};

pub struct ExprDisplay<'a> {
    expr: &'a Expr,
    objs: &'a AstObjects,
}

impl<'a> ExprDisplay<'a> {
    pub fn new(expr: &'a Expr, objs: &'a AstObjects) -> ExprDisplay<'a> {
        ExprDisplay {
            expr: expr,
            objs: objs,
        }
    }

    pub fn pos(&self) -> Pos {
        self.expr.pos(self.objs)
    }
}

impl<'a> fmt::Display for ExprDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt_expr(self.expr, f, self.objs)
    }
}

pub struct ExprIfaceDisplay<'a> {
    expr: &'a InterfaceType,
    objs: &'a AstObjects,
}

impl<'a> ExprIfaceDisplay<'a> {
    pub fn new(expr: &'a InterfaceType, objs: &'a AstObjects) -> ExprIfaceDisplay<'a> {
        ExprIfaceDisplay {
            expr: expr,
            objs: objs,
        }
    }
}

impl<'a> fmt::Display for ExprIfaceDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt_expr_interface(self.expr, f, self.objs)
    }
}

pub struct ExprCallDisplay<'a> {
    call: &'a CallExpr,
    objs: &'a AstObjects,
}

impl<'a> ExprCallDisplay<'a> {
    pub fn new(call: &'a CallExpr, objs: &'a AstObjects) -> ExprCallDisplay<'a> {
        ExprCallDisplay {
            call: call,
            objs: objs,
        }
    }

    pub fn pos(&self) -> Pos {
        self.call.func.pos(self.objs)
    }
}

impl<'a> fmt::Display for ExprCallDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt_expr_call(self.call, f, self.objs)
    }
}

pub struct LangObjDisplay<'a> {
    key: &'a ObjKey,
    objs: &'a TCObjects,
}

impl<'a> LangObjDisplay<'a> {
    pub fn new(key: &'a ObjKey, objs: &'a TCObjects) -> LangObjDisplay<'a> {
        LangObjDisplay {
            key: key,
            objs: objs,
        }
    }
}

impl<'a> fmt::Display for LangObjDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt_obj(self.key, f, self.objs)
    }
}

pub struct TypeDisplay<'a> {
    key: &'a TypeKey,
    objs: &'a TCObjects,
}

impl<'a> TypeDisplay<'a> {
    pub fn new(key: &'a TypeKey, objs: &'a TCObjects) -> TypeDisplay<'a> {
        TypeDisplay {
            key: key,
            objs: objs,
        }
    }
}

impl<'a> fmt::Display for TypeDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt_type(&Some(*self.key), f, self.objs)
    }
}

pub struct OperandDisplay<'a> {
    op: &'a Operand,
    ast_objs: &'a AstObjects,
    tc_objs: &'a TCObjects,
}

impl<'a> OperandDisplay<'a> {
    pub fn new(op: &'a Operand, aobjs: &'a AstObjects, tobjs: &'a TCObjects) -> OperandDisplay<'a> {
        OperandDisplay {
            op: op,
            ast_objs: aobjs,
            tc_objs: tobjs,
        }
    }

    pub fn pos(&self) -> Pos {
        self.op.pos(self.ast_objs)
    }
}

impl<'a> fmt::Display for OperandDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.op.fmt(f, self.tc_objs, self.ast_objs)
    }
}

pub struct MethodInfoDisplay<'a> {
    mi: &'a MethodInfo,
    ast_objs: &'a AstObjects,
    tc_objs: &'a TCObjects,
}

impl<'a> MethodInfoDisplay<'a> {
    pub fn new(
        mi: &'a MethodInfo,
        aobjs: &'a AstObjects,
        tobjs: &'a TCObjects,
    ) -> MethodInfoDisplay<'a> {
        MethodInfoDisplay {
            mi: mi,
            ast_objs: aobjs,
            tc_objs: tobjs,
        }
    }
}

impl<'a> fmt::Display for MethodInfoDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.mi.fmt(f, self.tc_objs, self.ast_objs)
    }
}
