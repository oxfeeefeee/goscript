use std::fmt;
use std::rc::Rc;
use std::cell::{RefCell};
use std::collections::HashMap;
use super::position;
use super::token::Token;
use super::scanner;
use super::errors;
use super::scope::*;
use super::ast::*;
use super::ast_objects::*;

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

    // ----------------------------------------------------------------------------
    // Scoping support

    fn open_scope(&mut self) {
        self.top_scope = Some(new_scope!(self, self.top_scope.take()));
    }

    fn close_scope(&mut self) {
        self.top_scope = scope!(self, self.top_scope.take().unwrap()).outer;
    }

    fn open_label_scope(&mut self) { 
        self.label_scope = 
            Some(new_scope!(self, self.label_scope.take()));
        self.target_stack.push(vec![]);
    }

    fn close_label_scope(&mut self) {
        let scope = scope!(self, *self.label_scope.as_ref().unwrap());
        match self.target_stack.pop() {
            Some(v) => {
                for i in v {
                    let ident = ident!(self, i);
                    if scope.look_up(&ident.name).is_none() {
                        let s = format!("label {} undefined", ident.name);
                        self.error(self.pos, s);
                    }
                }
            }
            _ => panic!("invalid target stack.")
        }
        self.label_scope = scope!(self, self.label_scope.take().unwrap()).outer;
    }

    fn declare(&mut self, decl: DeclObj, data: EntityData, kind: EntityKind,
        scope_ind: &ScopeIndex) {
        let mut names: Vec<IdentIndex> = vec![];
        let idents = match decl {
            DeclObj::Field(id) => &(field!(self, id).names),
            DeclObj::Spec(id) => { 
                match spec!(self, id) {
                    Spec::Value(vs) => &vs.names,
                    Spec::Type(ts) => {names.push(ts.name); &names},
                    Spec::Import(_) => &names,
                }},
            DeclObj::Decl(id) => { 
                match decl!(self, id) {
                    Decl::Func(f) => {names.push(f.name); &names},
                    _ => &names,
                }}, 
            DeclObj::Stmt(id) => {
                let stmt = stmt!(self, id);
                match stmt {
                    Stmt::Labeled(l) => {names.push(l.label); &names},
                    _ => &names,
                }},
            DeclObj::NoDecl => &names,
        };
        for id in idents.iter() {
            let mut_ident = ident_mut!(self, *id);
            let entity = new_entity!(self, kind.clone(), 
                mut_ident.name.clone(), decl.clone(), data.clone());
            mut_ident.entity = IdentEntity::Entity(entity);
            let ident = ident!(self, *id);
            if ident.name != "_" {
                let scope = scope_mut!(self, *scope_ind);
                match scope.insert(ident.name.clone(), entity) {
                    Some(prev_decl) => {
                        let p =  entity!(self, prev_decl).pos(&self.objects);
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

    fn short_var_decl(&mut self, assign_stmt: StmtIndex, list: Vec<Expr>) {
        // Go spec: A short variable declaration may redeclare variables
        // provided they were originally declared in the same block with
        // the same type, and at least one of the non-blank variables is new.
	    let mut n = 0; // number of new variables
        for expr in &list {
            match expr {
                Expr::Ident(id) => {
                    let ident = ident_mut!(self, *id.as_ref());
                    let entity = new_entity!(self, EntityKind::Var, 
                        ident.name.clone(), DeclObj::Stmt(assign_stmt),
                        EntityData::NoData);
                    ident.entity = IdentEntity::Entity(entity);
                    if ident.name != "_" {
                        let top_scope = scope_mut!(self, self.top_scope.unwrap());
                        match top_scope.insert(ident.name.clone(), entity) {
                            Some(e) => { ident.entity = IdentEntity::Entity(e); },
                            None => { n += 1; },
                        }
                    }
                },
                _ => {
                    self.error_expected(expr.pos(&self.objects), 
                        "identifier on left side of :=");
                },
            }
        }
        if n == 0 {
            self.error(list[0].pos(&self.objects), 
                "no new variables on left side of :=".to_string())
        }
    }

    // If x is an identifier, tryResolve attempts to resolve x by looking up
    // the object it denotes. If no object is found and collectUnresolved is
    // set, x is marked as unresolved and collected in the list of unresolved
    // identifiers.
    fn try_resolve(&mut self, x: &Expr, collect_unresolved: bool) {
        if let Expr::Ident(i) = x {
            let ident = ident_mut!(self, *i.as_ref());
            assert!(ident.entity.is_none(), 
                "identifier already declared or resolved");
            if ident.name == "_" {
                return;
            }
            // try to resolve the identifier
            let mut s = self.top_scope;
            loop {
                match s {
                    Some(sidx) => {
                        let scope = scope!(self, sidx);
                        if let Some(entity) = scope.look_up(&ident.name) {
                            ident.entity = IdentEntity::Entity(*entity);
                            return;
                        }
                        s = scope.outer;
                    },
                    None => {break;},
                }
            }
            // all local scopes are known, so any unresolved identifier
            // must be found either in the file scope, package scope
            // (perhaps in another file), or universe scope --- collect
            // them so that they can be resolved later
            if collect_unresolved {
                ident.entity = IdentEntity::Sentinel;
                self.unresolved.push(*i.as_ref());
            }
        }
    }

    fn resolve(&mut self, x: &Expr) {
        self.try_resolve(x, true)
    }

    // ----------------------------------------------------------------------------
    // Parsing support

    fn file_mut(&mut self) -> &mut position::File {
        self.scanner.file_mut()
    }

    fn file(&self) -> &position::File {
        self.scanner.file()
    }

    fn print_trace(&self, msg: &str) {
        let f = self.file();
        let p = f.position(self.pos);
        let mut buf = String::new();
        fmt::write(&mut buf, format_args!("{:5o}:{:3o}:", p.line, p.column)).unwrap();
        for _ in 0..self.indent {
            buf.push_str("..");
        }
        print!("{}{}\n", buf, msg);
    }

    fn trace_begin(&mut self, msg: &str) {
        if self.trace {
            let mut trace_str = msg.to_string();
            trace_str.push('(');
            self.print_trace(&trace_str);
            self.indent += 1;
        }
    }

    fn trace_end(&mut self) {
        if self.trace {
            self.indent -= 1;
            self.print_trace(")");
        }
    }

    fn next(&mut self) {
        // Print previous token
        if self.pos > 0 {
            self.print_trace(&format!("{}", self.token));
        }
        // Get next token and skip comments
        let mut token: Token;
        loop {
            token = self.scanner.scan();
            match token {
                Token::COMMENT(_) => { // Skip comment
                    self.print_trace(&format!("{}", self.token));
                },
                _ => { break; },
            }
        }
        self.token = token;
        self.pos = self.scanner.pos();
    }

    fn error(&self, pos: position::Pos, msg: String) {
        let p = self.file().position(pos);
        self.errors.borrow_mut().add(p, msg)
    }

    fn error_expected(&self, pos: position::Pos, msg: &str) {
        let mut mstr = "expected ".to_string();
        mstr.push_str(msg);
        if pos == self.pos {
            match self.token {
                Token::SEMICOLON(real) => if !real {
                    mstr.push_str(", found newline");
                },
                _ => {
                    mstr.push_str(", found ");
                    mstr.push_str(self.token.token_text());
                }
            }
        }
        self.error(pos, mstr);
    }

    fn expect(&mut self, token: &Token) -> position::Pos {
        let pos = self.pos;
        if self.token != *token {
            self.error_expected(pos, &format!("'{}'", token));
        }
        self.next();
        pos
    }

    // https://github.com/golang/go/issues/3008
    // Same as expect but with better error message for certain cases
    fn expect_closing(&mut self, token: &Token, context: &str) -> position::Pos {
        if let Token::SEMICOLON(real) = token {
            if !real {
                let msg = format!("missing ',' before newline in {}", context);
                self.error(self.pos, msg);
                self.next();
            }
        }
        self.expect(token)
    }

    fn expect_semi(&mut self) {
        // semicolon is optional before a closing ')' or '}'
        match self.token {
            Token::RPAREN | Token::RBRACE => {},
            Token::SEMICOLON(_) => { self.next(); },
            _ => {
                if let Token::COMMA = self.token {
                    // permit a ',' instead of a ';' but complain
                    self.error_expected(self.pos, "';'");
                    self.next();
                }
                self.error_expected(self.pos, "';'");
                self.sync_stmt();
            }
        }
    }

    fn at_comma(&self, context: &str, follow: &Token) -> bool {
        if let Token::COMMA = self.token {
            true
        } else if self.token == *follow {
            let mut msg =  "missing ','".to_string();
            if let Token::SEMICOLON(real) = self.token {
                if !real {msg.push_str(" before newline");}
            }
            msg = format!("{} in {}", msg, context);
            self.error(self.pos, msg);
            true
        } else {
            false
        }
    }

    // syncStmt advances to the next statement.
    // Used for synchronization after an error.
    fn sync_stmt(&mut self) {
        loop {
            match self.token {
                Token::BREAK | Token::CONST | Token::CONTINUE | Token::DEFER |
			    Token::FALLTHROUGH | Token::FOR | Token::GO | Token::GOTO | 
			    Token::IF | Token::RETURN | Token::SELECT | Token::SWITCH |
			    Token::TYPE | Token::VAR => {
                    // Return only if parser made some progress since last
                    // sync or if it has not reached 10 sync calls without
                    // progress. Otherwise consume at least one token to
                    // avoid an endless parser loop (it is possible that
                    // both parseOperand and parseStmt call syncStmt and
                    // correctly do not advance, thus the need for the
                    // invocation limit p.syncCnt).
                    if self.pos == self.sync_pos && self.sync_count < 10 {
                        self.sync_count += 1;
                        return;
                    }
                    if self.pos > self.sync_pos {
                        self.sync_pos = self.pos;
                        self.sync_count = 0;
                        return;
                    }
                },
                // Reaching here indicates a parser bug, likely an
                // incorrect token list in this function, but it only
                // leads to skipping of possibly correct code if a
                // previous error is present, and thus is preferred
                // over a non-terminating parse.
                Token::EOF => { return; },
                _ => {},
            }
            self.next();
        }
    }

    // syncDecl advances to the next declaration.
    // Used for synchronization after an error.
    fn sync_decl(&mut self) {
        loop {
            match self.token {
                Token::CONST | Token::TYPE | Token::VAR => {
                    // same as sync_stmt
                    if self.pos == self.sync_pos && self.sync_count < 10 {
                        self.sync_count += 1;
                        return;
                    }
                    if self.pos > self.sync_pos {
                        self.sync_pos = self.pos;
                        self.sync_count = 0;
                        return;
                    }
                }
                Token::EOF => { return; },
                _ => {},
            }
            self.next();
        }
    }

    // safe_pos returns a valid file position for a given position: If pos
    // is valid to begin with, safe_pos returns pos. If pos is out-of-range,
    // safe_pos returns the EOF position.
    //
    // This is hack to work around "artificial" end positions in the AST which
    // are computed by adding 1 to (presumably valid) token positions. If the
    // token positions are invalid due to parse errors, the resulting end position
    // may be past the file's EOF position, which would lead to panics if used
    // later on.
    fn safe_pos(&self, pos: position::Pos) -> position::Pos {
        let max = self.file().base() + self.file().size(); 
        if pos > max { max } else { pos }
    }

    // ----------------------------------------------------------------------------
    // Identifiers

    fn parse_ident(&mut self) -> IdentIndex {
        let pos = self.pos;
        let mut name = "_".to_string();
        if let Token::IDENT(lit) = self.token.clone() {
            name = lit;
            self.next();
        } else {
            self.expect(&Token::IDENT("".to_string()));
        }
        self.objects.idents.insert(Ident{ pos: pos, name: name,
            entity: IdentEntity::NoEntity})
    }

    fn parse_ident_list(&mut self) -> Vec<IdentIndex> {
        self.trace_begin("IdentList");
        
        let mut list = vec![self.parse_ident()];
        while self.token == Token::COMMA {
            self.next();
            list.push(self.parse_ident());
        }
       
        self.trace_end();
        list
    }

    // ----------------------------------------------------------------------------
    // Common productions
    fn parse_expr_list(&mut self, lhs: bool) -> Vec<Expr> {
        self.trace_begin("ExpressionList");

        let expr = self.parse_expr(lhs);
        let mut list = vec![self.check_expr(expr)];
        while self.token == Token::COMMA {
            self.next();
            let expr = self.parse_expr(lhs);
            list.push(self.check_expr(expr));
        }

        self.trace_end();
        list
    }

    fn parse_lhs_list(&mut self) -> Vec<Expr> {
        let bak = self.in_rhs;
        self.in_rhs = false;
        let list = self.parse_expr_list(true);
        match self.token {
            // lhs of a short variable declaration
            // but doesn't enter scope until later:
            // caller must call self.short_var_decl(list)
            // at appropriate time.
            Token::DEFINE => {},
            // lhs of a label declaration or a communication clause of a select
            // statement (parse_lhs_list is not called when parsing the case clause
            // of a switch statement):
            // - labels are declared by the caller of parse_lhs_list
            // - for communication clauses, if there is a stand-alone identifier
            //   followed by a colon, we have a syntax error; there is no need
            //   to resolve the identifier in that case
            Token::COLON => {},
            _ => {
                // identifiers must be declared elsewhere
                for x in list.iter() {
                    self.resolve(x);
                }
            }
        }
        self.in_rhs = bak;
        list
    }

    fn parse_rhs_list(&mut self) -> Vec<Expr> {
        let bak = self.in_rhs;
        self.in_rhs = true;
        let list = self.parse_expr_list(false);
        self.in_rhs = bak;
        list
    }

    // ----------------------------------------------------------------------------
    // Types
    fn parse_type(&mut self) -> Expr {
        self.trace_begin("Type");

        let typ = self.try_type();
        let ret = if typ.is_none() {
            let pos = self.pos;
            self.error_expected(pos, "type");
            self.next();
            Expr::new_bad(pos, self.pos)
        } else {
            typ.unwrap()
        };
       
        self.trace_end();
        ret
    }
    
    // If the result is an identifier, it is not resolved.
    fn parse_type_name(&mut self) -> Expr {
        self.trace_begin("TypeName");

        let ident = self.parse_ident();
        let x_ident = Expr::Ident(Box::new(ident));
        // don't resolve ident yet - it may be a parameter or field name
        let ret = if let Token::PERIOD = self.token {
            // ident is a package name
            self.next();
            self.resolve(&x_ident);
            let sel = self.parse_ident();
            Expr::new_selector(x_ident, sel)
        } else {
            x_ident
        };

        self.trace_end();
        ret
    }

    fn parse_array_type(&mut self) -> Expr {
        self.trace_begin("ArrayType");

        let lpos = self.expect(&Token::LBRACK);
        self.expr_level += 1;
        let len = match self.token {
            // always permit ellipsis for more fault-tolerant parsing
            Token::ELLIPSIS => {
                Some(Expr::new_ellipsis(self.pos, None))
            },
            _ if self.token != Token::RBRACK => {
                Some(self.parse_rhs())
            },
            _ => None,
        };
        self.expr_level -= 1;
        self.expect(&Token::RBRACK);
        let elt = self.parse_type();

        self.trace_end();
        Expr::Array(Box::new(ArrayType{
            l_brack: lpos, len: len, elt: elt}))
    }

    fn make_ident_list(&mut self, exprs: &mut Vec<Expr>) -> Vec<IdentIndex> {
        exprs.iter().map(|x| {
            match x {
                Expr::Ident(ident) => *ident.as_ref(),
                _ => {
                    let pos = x.pos(&self.objects);
                    if let Expr::Bad(_) = x {
                        // only report error if it's a new one
                        self.error_expected(pos, "identifier")
                    }
                    new_ident!(self, pos, "_".to_string(), IdentEntity::NoEntity)
                }
            }
        }).collect()
    }

    
    fn parse_field_decl(&mut self, scope: ScopeIndex) -> FieldIndex {
        self.trace_begin("FieldDecl");

        // 1st FieldDecl
	    // A type name used as an anonymous field looks like a field identifier.
        let mut list = vec![];
        loop {
            list.push(self.parse_var_type(false));
            if let Token::COMMA = self.token {
                break;
            }
            self.next();
        }

        let mut idents = vec![];
        let typ = match self.try_var_type(false) {
            Some(t) => {
                idents = self.make_ident_list(&mut list);
                t
            }
            // ["*"] TypeName (AnonymousField)
            None => { 
                let first = &list[0]; // we always have at least one element
                if list.len() > 1 {
                    self.error_expected(self.pos, "type");
                    Expr::new_bad(self.pos, self.pos)
                } else if !Parser::is_type_name(Parser::deref(first)) {
                    self.error_expected(self.pos, "anonymous field");
                    Expr::new_bad(
                        first.pos(&self.objects),
                        self.safe_pos(first.end(&self.objects)))
                } else {
                    list.into_iter().nth(0).unwrap()
                }
            }
        };

        // Tag
        let token = self.token.clone();
        let tag = if let Token::STRING(s) = token {
            self.next();
            Some(Expr::new_basic_lit(self.pos, self.token.clone(), s.clone()))
        } else {
            None
        };

        self.expect_semi();

        // have to clone to fix ownership issue.
        let field = new_field!(self, idents, typ.clone_ident(), tag);
        self.declare(DeclObj::Field(field), EntityData::NoData,
            EntityKind::Var, &scope);
        self.resolve(&typ);

        self.trace_end();
        field
    }

    fn parse_struct_type(&mut self) -> Expr {
        self.trace_begin("FieldDecl");

        let stru = self.expect(&Token::STRUCT);
        let lbrace = self.expect(&Token::LBRACE);
        let scope = new_scope!(self, None);
        let mut list = vec![];
        loop {
            match &self.token {
                Token::IDENT(_) | Token::MUL | Token::LPAREN => {
                    list.push(self.parse_field_decl(scope));
                }
                _ => {break;}
            } 
        }
        let rbrace = self.expect(&Token::RBRACE);

        self.trace_end();
        Expr::Struct(Box::new(StructType{
            struct_pos: stru,
            fields: FieldList::new(Some(lbrace), list, Some(rbrace)),
            incomplete: false,
        }))
    }

    fn parse_pointer_type(&mut self) -> Expr {
        self.trace_begin("PointerType");

        let star = self.expect(&Token::MUL);
        let base = self.parse_type();

        self.trace_end();
        Expr::Star(Box::new(StarExpr{star: star, expr: base}))
    }

    // If the result is an identifier, it is not resolved.
    fn try_var_type(&mut self, is_param: bool) -> Option<Expr> {
        if is_param {
            if let Token::ELLIPSIS = self.token {
                let pos = self.pos;
                self.next();
                let typ = if let Some(t) = self.try_ident_or_type() {
                    self.resolve(&t);
                    t
                    
                } else {
                    self.error(pos, "'...' parameter is missing type".to_string());
                    Expr::new_bad(pos, self.pos)
                };
                return Some(Expr::new_ellipsis(pos, Some(typ)));
            }
        }
        self.try_ident_or_type()
    }

    fn parse_var_type(&mut self, is_param: bool) -> Expr {
        match self.try_var_type(is_param) {
            Some(typ) => typ,
            None => {
                let pos = self.pos;
                self.error_expected(pos, "type");
                self.next();
                Expr::new_bad(pos, self.pos)
            },
        }
    }

    fn parse_parameter_list(&mut self, scope: ScopeIndex,
        ellipsis_ok: bool) -> Vec<FieldIndex> {
        self.trace_begin("ParameterList");

        // 1st ParameterDecl
	    // A list of identifiers looks like a list of type names.
        let mut list = vec![];
        loop {
            list.push(self.parse_var_type(ellipsis_ok));
            if let Token::COMMA = &self.token {
                break;
            }
            self.next();
            if let Token::RPAREN = &self.token {
                break;
            }
        }

        let mut params = vec![];
        let typ = self.try_var_type(ellipsis_ok);
        if let Some(t) = typ {
            // IdentifierList Type
            let idents = self.make_ident_list(&mut list);
            let field = new_field!(self, idents, t.clone_ident(), None);
            params.push(field);
            // Go spec: The scope of an identifier denoting a function
			// parameter or result variable is the function body.
			self.declare(DeclObj::Field(field), EntityData::NoData,
                EntityKind::Var, &scope);
            self.resolve(&t);
            if !self.at_comma("parameter list", &Token::RPAREN) {
                self.trace_end();
                return params;
            }
            self.next();
            loop {
                let idents = self.parse_ident_list();
                let t = self.parse_var_type(ellipsis_ok);
                let field = new_field!(self, idents, t.clone_ident(), None);
                // warning: copy paste
                params.push(field);
                // Go spec: The scope of an identifier denoting a function
                // parameter or result variable is the function body.
                self.declare(DeclObj::Field(field), EntityData::NoData,
                    EntityKind::Var, &scope);
                self.resolve(&t);
                if !self.at_comma("parameter list", &Token::RPAREN) {
                    break;
                }
                self.next();
            }
        } else {
            // Type { "," Type } (anonymous parameters)
            for typ in list {
                self.resolve(&typ);
                params.push(new_field!(self, vec![], typ, None));
            }
        }
        self.trace_end();
        params
    }

    fn parse_parameters(&mut self, scope: ScopeIndex,
        ellipsis_ok: bool) -> FieldList {
        self.trace_begin("Parameters");

        let mut params = vec![];
        let lparen = Some(self.expect(&Token::LPAREN));
        if self.token != Token::RPAREN {
            params = self.parse_parameter_list(scope, ellipsis_ok);
        }
        let rparen = Some(self.expect(&Token::RPAREN));

        self.trace_end();
        FieldList::new(lparen, params, rparen)
    }

    fn parse_result(&mut self, scope: ScopeIndex) -> FieldList {
        self.trace_begin("Result");

        let ret = if self.token == Token::LPAREN {
            self.parse_parameters(scope, false)
        } else {
            if let Some(t) = self.try_type() {
                let field = new_field!(self, vec![], t, None);
                FieldList::new(None, vec![field], None)
            } else {
                FieldList::new(None, vec![], None)
            }
        };

        self.trace_end();
        ret
    }

    fn parse_signature(&mut self, scope: ScopeIndex) -> (FieldList, FieldList) {
        self.trace_begin("Result");

        let params = self.parse_parameters(scope, true);
        let results = self.parse_result(scope);

        self.trace_end();   
        (params, results)
    }

    fn parse_func_type(&mut self) -> (Expr, ScopeIndex) {
        self.trace_begin("FuncType");

        let pos = self.expect(&Token::FUNC);
        let scope = new_scope!(self, self.top_scope);
        let (params, results) = self.parse_signature(scope);

        self.trace_end();
        (Expr::new_func_type(Some(pos), params, Some(results)), scope)
    }

    // method spec in interface
    fn parse_method_spec(&mut self, scope: ScopeIndex) -> FieldIndex {
        self.trace_begin("MethodSpec");

        let mut idents = vec![];
        let mut typ = self.parse_type_name();
        let ident = typ.unwrap_ident().clone();
        if let Token::LPAREN = self.token {
            idents = vec![ident];
            let scope = new_scope!(self, self.top_scope);
            let (params, results) = self.parse_signature(scope);
            typ = Expr::new_func_type(None, params, Some(results))
        } else {
            // embedded interface
            self.resolve(&typ);
        }
        self.expect_semi();
        let field = new_field!(self, idents, typ, None);
        self.declare(DeclObj::Field(field), EntityData::NoData, EntityKind::Fun, &scope);

        self.trace_end();
        field
    }

    //todo
    fn try_ident_or_type(&mut self) -> Option<Expr> {
        None
    }

    fn try_type(&mut self) -> Option<Expr> {
        if let Some(typ) = self.try_ident_or_type() {
            self.resolve(&typ);
            Some(typ)
        } else {
            None
        }
    }

    // ----------------------------------------------------------------------------
    // Expressions

    // checkExpr checks that x is an expression (and not a type).
    fn check_expr(&self, x: Expr) -> Expr {
        match x {
            Expr::Bad(_) => x,
            Expr::Ident(_) => x,
            Expr::BasicLit(_) => x,
            Expr::FuncLit(_) => x,
            Expr::CompositeLit(_) => x,
            Expr::Paren(_) => { panic!("unreachable"); },
            Expr::Selector(_) => x,
            Expr::Index(_) => x,
            Expr::Slice(_) => x,
            // If t.Type == nil we have a type assertion of the form
            // y.(type), which is only allowed in type switch expressions.
            // It's hard to exclude those but for the case where we are in
            // a type switch. Instead be lenient and test this in the type
            // checker.
            Expr::TypeAssert(_) => x,
            Expr::Call(_) => x,
            Expr::Star(_) => x,
            Expr::Unary(_) => x,
            Expr::Binary(_) => x,
            _ => {
                self.error_expected(self.pos, "expression");
                Expr::new_bad(
                    x.pos(&self.objects), 
                    self.safe_pos(x.end(&self.objects)))
            }
        }
    }

    // isTypeName reports whether x is a (qualified) TypeName.
    fn is_type_name(x: &Expr) -> bool {
        match x {
            Expr::Bad(_) | Expr::Ident(_) => true,
            Expr::Selector(s) => {
                if let Expr::Ident(_) = s.expr {true} else {false}
            },
            _ => false
        }
    }

    // isLiteralType reports whether x is a legal composite literal type.
    fn is_literal_type(x: &Expr) -> bool {
        match x {
            Expr::Bad(_) | Expr::Ident(_)  | Expr::Array(_) |
            Expr::Struct(_) | Expr::Map(_) => true,
            Expr::Selector(s) => {
                if let Expr::Ident(_) = s.expr {true} else {false}
            },
            _ => false
        }
    }

    fn deref(x: &Expr) -> &Expr {
        if let Expr::Star(s) = x {&s.expr} else {x}
    }

    // todo
    fn parse_expr(&mut self, _lhs: bool) -> Expr {
        Expr::new_bad(0, 0)
    }

    fn parse_rhs(&mut self) -> Expr {
        let bak = self.in_rhs;
        self.in_rhs = true;
        let x0 = self.parse_expr(false);
        let x1 = self.check_expr(x0);
        self.in_rhs = bak;
        x1
    }

    
    fn parse(&mut self) {
        self.trace_begin("begin");
        print!("222xxxxxxx \n");
        self.trace_end();
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