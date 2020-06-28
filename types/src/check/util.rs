#![allow(dead_code)]

use super::super::objects::{ObjKey, TypeKey};
use super::super::operand::{Operand, OperandMode};
use super::super::typ::BasicType;
use super::check::Checker;
use goscript_parser::{ast::Expr, position::Pos};

pub enum UnpackResult<'a> {
    Tuple(Option<Expr>, Vec<TypeKey>),   // rhs is a tuple
    CommaOk(Option<Expr>, Vec<TypeKey>), // rhs returns comma_ok
    Mutliple(&'a Vec<Expr>),             // M to N
    Single(Operand),                     // 1 to 1
    Error,                               // errors when trying to unpack
}

impl<'a> UnpackResult<'a> {
    pub fn len(&self) -> usize {
        match self {
            UnpackResult::Tuple(_, types) => types.len(),
            UnpackResult::CommaOk(_, types) => types.len(),
            UnpackResult::Mutliple(exprs) => exprs.len(),
            UnpackResult::Single(_) => 1,
            UnpackResult::Error => 0,
        }
    }

    pub fn get(&self, checker: &mut Checker, x: &mut Operand, i: usize) {
        match self {
            UnpackResult::Tuple(expr, types) => {
                x.mode = OperandMode::Value;
                x.expr = expr.clone();
                x.typ = Some(types[i]);
            }
            UnpackResult::CommaOk(expr, types) => {
                x.mode = OperandMode::Value;
                x.expr = expr.clone();
                x.typ = Some(types[i]);
            }
            UnpackResult::Mutliple(exprs) => {
                checker.multi_expr(x, &exprs[i]);
            }
            UnpackResult::Single(sx) => {
                x.mode = sx.mode.clone();
                x.expr = sx.expr.clone();
                x.typ = sx.typ;
            }
            UnpackResult::Error => unreachable!(),
        }
    }
}

impl<'a> Checker<'a> {
    /// invalid_ast helps to report ast error
    pub fn invalid_ast(&self, pos: Pos, err: &str) {
        self.error(pos, format!("invalid AST: {}", err));
    }

    pub fn obj_path_str(&self, path: &Vec<ObjKey>) -> String {
        let names: Vec<&str> = path.iter().map(|p| self.lobj(*p).name().as_str()).collect();
        names[..].join("->")
    }

    pub fn print_trace(&self, pos: Pos, msg: &str) {
        let file = self.fset.file(pos).unwrap();
        let p = file.position(pos);
        print!("{}:\t{}{}\n", p, ".  ".repeat(self.indent), msg);
    }

    pub fn trace_begin(&mut self, pos: Pos, msg: &str) {
        self.print_trace(pos, msg);
        self.indent += 1;
    }

    pub fn trace_end(&mut self, pos: Pos, msg: &str) {
        self.indent -= 1;
        self.print_trace(pos, msg);
    }

    // has_cycle reports whether obj appears in path or not.
    // If it does, and report is set, it also reports a cycle error.
    pub fn has_cycle(&self, okey: ObjKey, path: &Vec<ObjKey>, report: bool) -> bool {
        if let Some((i, _)) = path.iter().enumerate().find(|(_, &x)| x == okey) {
            if report {
                let obj_val = self.lobj(okey);
                self.error(
                    *obj_val.pos(),
                    format!("illegal cycle in declaration of {}", obj_val.name()),
                );
                // print cycle
                for o in path[i..].iter() {
                    let oval = self.lobj(*o);
                    self.error(*oval.pos(), format!("\t{} refers to", oval.name()));
                }
                self.error(*obj_val.pos(), format!("\t{}", obj_val.name()));
            }
            return true;
        }
        false
    }

    pub fn unpack<'b>(&mut self, rhs: &'b Vec<Expr>, allow_comma_ok: bool) -> UnpackResult<'b> {
        if rhs.len() != 1 {
            return UnpackResult::Mutliple(rhs);
        }

        let mut x = Operand::new();
        self.multi_expr(&mut x, &rhs[0]);
        if x.mode == OperandMode::Invalid {
            return UnpackResult::Error;
        }

        if let Some(t) = self.otype(x.typ.unwrap()).try_as_tuple() {
            let types = t
                .vars()
                .iter()
                .map(|x| self.lobj(*x).typ().unwrap())
                .collect();
            return UnpackResult::Tuple(x.expr.clone(), types);
        } else if x.mode == OperandMode::MapIndex || x.mode == OperandMode::CommaOk {
            if allow_comma_ok {
                let types = vec![x.typ.unwrap(), self.basic_type(BasicType::UntypedBool)];
                return UnpackResult::CommaOk(x.expr.clone(), types);
            }
            x.mode = OperandMode::Value;
        }

        UnpackResult::Single(x)
    }
}
