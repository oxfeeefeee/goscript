#![allow(dead_code)]

use super::super::objects::ObjKey;
use super::check::Checker;
use goscript_parser::position::Pos;

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
}
