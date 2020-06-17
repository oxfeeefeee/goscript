use super::super::objects::ObjKey;
use super::super::operand::Operand;
use super::check::Checker;
use goscript_parser::ast::Expr;
use goscript_parser::position::Pos;

impl<'a> Checker<'a> {
    pub fn init_const(&mut self, okey: ObjKey, x: &mut Operand) {
        unimplemented!()
    }

    pub fn init_var(&mut self, okey: ObjKey, x: &mut Operand, msg: &str) {
        unimplemented!()
    }

    pub fn init_vars(&mut self, lhs: &Vec<ObjKey>, rhs: &Vec<Expr>, ret_pos: Pos) {
        unimplemented!()
    }
}
