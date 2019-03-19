use std::fmt;
use std::rc::Rc;
use std::cell::{Ref, RefMut, RefCell};
use super::position;
use super::token;
use super::token::Token;
use super::scanner;
use super::errors;
use super::scope::*;
use super::ast;
use super::ast_objects::*;
use super::helpers::{Defer};

macro_rules! trace {
    ($self:ident, $msg:expr) => {
        if $self.trace {
            let mut trace_str = $msg.to_string();
            trace_str.push('(');
            $self.print_trace(&trace_str);
            $self.indent += 1;
        }
        let _defer_ins = Defer::new(|| {
            if $self.trace {
                $self.indent -= 1;
                $self.print_trace(")");
            }
        });
    };
}

pub struct Parser<'a> {
    objects: Objects,
    scanner: scanner::Scanner<'a>,
    errors: Rc<RefCell<errors::ErrorList>>,

    trace: bool,
    indent: isize,

    pos: position::Pos,
    token: Token,

    sync_pos: position::Pos,
    sync_count: isize,

    expr_level: isize,
    in_rhs: bool,

    pkg_scope: Option<ScopeIndex>,
    top_scope: Option<ScopeIndex>,
    unresolved: Vec<IdentIndex>,
    imports: Vec<SpecIndex>, //ImportSpec

    label_scope: Option<ScopeIndex>,
    target_stack: Vec<Vec<IdentIndex>>,
}

impl<'a> Parser<'a> {
    fn new(file: &'a mut position::File, src: &'a str, trace: bool) -> Parser<'a> {
        let err = Rc::new(RefCell::new(errors::ErrorList::new()));
        let s = scanner::Scanner::new(file, src, err.clone());
        Parser{
            objects: Objects::new(),
            scanner: s,
            errors: err,
            trace: trace,
            indent: 0,
            pos: 0,
            token: Token::ILLEGAL("".to_string()),
            sync_pos: 0,
            sync_count: 0,
            expr_level: 0,
            in_rhs: false,
            pkg_scope: None,
            top_scope: None,
            unresolved: vec![],
            imports: vec![],
            label_scope:None,
            target_stack: vec![],
        }
    }

    fn open_scope(&mut self) {
        self.top_scope = 
            Some(Scope::arena_new(self.top_scope.take(), &mut self.objects.scopes));
    }

    fn close_scope(&mut self) {
        self.top_scope = self.objects.scopes[self.top_scope.take().unwrap()].outer;
    }

    fn open_label_scope(&mut self) {
        self.label_scope = 
            Some(Scope::arena_new(self.label_scope.take(), &mut self.objects.scopes));
        self.target_stack.push(vec![]);
    }

    fn close_label_scope(&mut self) {
        let scope = &self.objects.scopes[*self.label_scope.as_ref().unwrap()];
        match self.target_stack.pop() {
            Some(v) => {
                for i in v {
                    let ident = &self.objects.idents[i];
                    if scope.look_up(&ident.name, &mut self.objects.entities).is_none() {
                        let s = format!("label {} undefined", ident.name);
                        self.error(self.pos, s);
                    }
                }
            }
            _ => panic!("invalid target stack.")
        }
        self.label_scope = self.objects.scopes[self.label_scope.take().unwrap()].outer;
    }

    fn declare(&mut self, decl: DeclObj, data: EntityData, kind: EntityKind,
        scope_ind: &ScopeIndex, idents: Vec<IdentIndex>) {
        for id in idents.iter() {
            let mut_ident = &mut self.objects.idents[*id];
            let entity = Entity::arena_new(kind.clone(), mut_ident.name.clone(),
                decl.clone(), data.clone(), &mut self.objects.entities);
            mut_ident.entity = entity;
            let ident = &self.objects.idents[*id];
            if ident.name != "_" {
                let scope = &mut self.objects.scopes[*scope_ind];
                match scope.insert(ident.name.clone(), entity) {
                    Some(prev_decl) => {
                        let p =  self.objects.entities[prev_decl].pos(&self.objects);
                        let mut buf = String::new();
                        fmt::write(&mut buf, format_args!(
                            "{} redeclared in this block\n\tprevious declaration at {}",
                            ident.name, 
                            self.file().position(p))).unwrap();
                        self.error(ident.pos, buf);
                    },
                    _ => {},
                }
            }
        }
    }

    fn short_var_decl(assign_stmt: StmtIndex, list: Vec<ast::Expr>) {
        // Go spec: A short variable declaration may redeclare variables
        // provided they were originally declared in the same block with
        // the same type, and at least one of the non-blank variables is new.
	    let n = 0; // number of new variables
    }

    fn error(&self, pos: position::Pos, msg: String) {
        let p = self.file().position(pos);
        self.errors.borrow_mut().add(p, msg)
    }

    fn error_expected(&self, pos: position::Pos, msg: &String) {
        let mut mstr = "expected ".to_string();
        mstr.push_str(msg);
        if pos == self.pos {
            match self.token {
                Token::SEMICOLON(b) => {
                    if b { mstr.push_str(", found newline"); };
                },
                _ => {
                    mstr.push_str(", found ");
                    mstr.push_str(self.token.token_text());
                }
            }
        }
        self.error(pos, mstr);
    }

    fn print_trace(&self, msg: &str) {
        let f = self.file();
        let p = f.position(self.pos);
        let mut buf = String::new();
        fmt::write(&mut buf, format_args!("{:5o}:{:3o}:", p.line, p.column)).unwrap();
        for i in 0..self.indent {
            buf.push_str("..");
        }
        print!("{}{}\n", buf, msg);
    }
    
    fn parse(&mut self) {
        trace!(self, "aa");
        print!("222\n");
    }
    
    fn file_mut(&mut self) -> &mut position::File {
        self.scanner.file_mut()
    }

    fn file(&self) -> &position::File {
        self.scanner.file()
    }
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_parser () {
        let fs = position::SharedFileSet::new();
        let mut fsm = fs.borrow_mut();
        let f = fsm.add_file(fs.weak(), "testfile1.gs", 0, 1000);

        let mut p = Parser::new(f, "", true);
        p.parse();
    }
}