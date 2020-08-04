#![allow(dead_code)]
use super::constant;
use super::lookup::missing_method;
use super::objects::{TCObjects, TypeKey};
use super::typ;
use super::typ::{fmt_type, BasicType, Type};
use super::universe::{Builtin, Universe};
use goscript_parser::ast;
use goscript_parser::ast::*;
use goscript_parser::objects::{FuncTypeKey, IdentKey, Objects as AstObjects};
use goscript_parser::position;
use goscript_parser::token::Token;
use goscript_parser::visitor::{walk_expr, ExprVisitor};
use std::fmt;
use std::fmt::Display;
use std::fmt::Write;

/// An OperandMode specifies the (addressing) mode of an operand.
#[derive(Clone, Debug, PartialEq)]
pub enum OperandMode {
    Invalid,                   // operand is invalid
    NoValue,                   // operand represents no value (result of a function call w/o result)
    Builtin(Builtin),          // operand is a built-in function
    TypeExpr,                  // operand is a type
    Constant(constant::Value), // operand is a constant; the operand's typ is a Basic type
    Variable,                  // operand is an addressable variable
    MapIndex, // operand is a map index expression (acts like a variable on lhs, commaok on rhs of an assignment)
    Value,    // operand is a computed value
    CommaOk,  // like value, but operand may be used in a comma,ok expression
}

impl OperandMode {
    pub fn constant_val(&self) -> Option<&constant::Value> {
        match self {
            OperandMode::Constant(v) => Some(v),
            _ => None,
        }
    }

    pub fn builtin_id(&self) -> Option<Builtin> {
        match self {
            OperandMode::Builtin(id) => Some(*id),
            _ => None,
        }
    }
}

impl fmt::Display for OperandMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            OperandMode::Invalid => "invalid operand",
            OperandMode::NoValue => "no value",
            OperandMode::Builtin(_) => "built-in",
            OperandMode::TypeExpr => "type",
            OperandMode::Constant(_) => "constant",
            OperandMode::Variable => "variable",
            OperandMode::MapIndex => "map index expression",
            OperandMode::Value => "value",
            OperandMode::CommaOk => "comma, ok expression",
        })
    }
}

/// An Operand represents an intermediate value during type checking.
/// Operands have an (addressing) mode, the expression evaluating to
/// the operand, the operand's type, a value for constants, and an id
/// for built-in functions.
#[derive(Clone, Debug)]
pub struct Operand {
    pub mode: OperandMode,
    pub expr: Option<ast::Expr>,
    pub typ: Option<TypeKey>,
}

impl Operand {
    pub fn new() -> Operand {
        Operand::new_with(OperandMode::Invalid, None, None)
    }

    pub fn new_with(mode: OperandMode, expr: Option<ast::Expr>, typ: Option<TypeKey>) -> Operand {
        Operand {
            mode: mode,
            expr: expr,
            typ: typ,
        }
    }

    pub fn invalid(&self) -> bool {
        self.mode == OperandMode::Invalid
    }

    pub fn pos(&self, ast_objs: &AstObjects) -> position::Pos {
        if let Some(e) = &self.expr {
            e.pos(ast_objs)
        } else {
            0
        }
    }

    pub fn set_const(&mut self, t: &Token, u: &Universe) {
        let bt = match t {
            Token::INT(_) => BasicType::UntypedInt,
            Token::FLOAT(_) => BasicType::UntypedFloat,
            Token::IMAG(_) => BasicType::UntypedComplex,
            Token::CHAR(_) => BasicType::UntypedRune,
            Token::STRING(_) => BasicType::UntypedString,
            _ => unreachable!(),
        };
        self.mode = OperandMode::Constant(constant::Value::with_literal(t));
        self.typ = Some(u.types()[&bt]);
    }

    pub fn is_nil(&self, u: &Universe) -> bool {
        match self.mode {
            OperandMode::Value => self.typ == Some(u.types()[&BasicType::UntypedNil]),
            _ => false,
        }
    }

    /// assignable_to returns whether self is assignable to a variable of type 't'.
    /// If the result is false and a non-None reason is provided, it may be set
    /// to a more detailed explanation of the failure.
    pub fn assignable_to(&self, t: TypeKey, reason: Option<&mut String>, objs: &TCObjects) -> bool {
        let u = objs.universe();
        if self.invalid() || t == u.types()[&BasicType::Invalid] {
            return true; // avoid spurious errors
        }

        if typ::identical(self.typ.unwrap(), t, objs) {
            return true;
        }

        let (k_left, k_right) = (t, self.typ.unwrap());
        let t_left = &objs.types[k_left];
        let t_right = &objs.types[k_right];
        let ut_key_left = typ::underlying_type(k_left, objs);
        let ut_key_right = typ::underlying_type(k_right, objs);
        let ut_left = &objs.types[ut_key_left];
        let ut_right = &objs.types[ut_key_right];

        if ut_right.is_untyped(objs) {
            match ut_left {
                Type::Basic(detail) => {
                    if self.is_nil(u) && detail.typ() == BasicType::UnsafePointer {
                        return true;
                    }
                    if let OperandMode::Constant(val) = &self.mode {
                        return val.representable(detail, None);
                    }
                    // The result of a comparison is an untyped boolean,
                    // but may not be a constant.
                    if detail.typ() == BasicType::Bool {
                        return ut_right.try_as_basic().unwrap().typ() == BasicType::UntypedBool;
                    }
                }
                Type::Interface(detail) => return self.is_nil(u) || detail.is_empty(),
                Type::Pointer(_)
                | Type::Signature(_)
                | Type::Slice(_)
                | Type::Map(_)
                | Type::Chan(_) => return self.is_nil(u),
                _ => {}
            }
        }

        // 'right' is typed
        // self's type 'right' and 'left' have identical underlying types
        // and at least one of 'right' or 'left' is not a named type
        if typ::identical(ut_key_right, ut_key_left, objs)
            && (!t_right.is_named() || !t_left.is_named())
        {
            return true;
        }

        // 'left' is an interface and 'right' implements 'left'
        if let Some(_) = ut_left.try_as_interface() {
            if let Some((m, wrong_type)) = missing_method(k_right, ut_key_left, true, objs) {
                if let Some(re) = reason {
                    let msg = if wrong_type {
                        "wrong type for method"
                    } else {
                        "missing method"
                    };
                    *re = format!("{} {}", msg, objs.lobjs[m].name());
                }
                return false;
            }
            return true;
        }

        // 'right' is a bidirectional channel value, 'left' is a channel
        // type, they have identical element types,
        // and at least one of 'right' or 'left' is not a named type
        if let Some(cr) = ut_right.try_as_chan() {
            if cr.dir() == typ::ChanDir::SendRecv {
                if let Some(cl) = ut_left.try_as_chan() {
                    if typ::identical(cr.elem(), cl.elem(), objs) {
                        return !t_right.is_named() || !t_left.is_named();
                    }
                }
            }
        }

        false
    }

    /// Operand string formats
    /// (not all "untyped" cases can appear due to the type system)
    ///
    /// mode       format
    ///
    /// invalid    <expr> (               <mode>                    )
    /// novalue    <expr> (               <mode>                    )
    /// builtin    <expr> (               <mode>                    )
    /// typexpr    <expr> (               <mode>                    )
    ///
    /// constant   <expr> (<untyped kind> <mode>                    )
    /// constant   <expr> (               <mode>       of type <typ>)
    /// constant   <expr> (<untyped kind> <mode> <val>              )
    /// constant   <expr> (               <mode> <val> of type <typ>)
    ///
    /// variable   <expr> (<untyped kind> <mode>                    )
    /// variable   <expr> (               <mode>       of type <typ>)
    ///
    /// mapindex   <expr> (<untyped kind> <mode>                    )
    /// mapindex   <expr> (               <mode>       of type <typ>)
    ///
    /// value      <expr> (<untyped kind> <mode>                    )
    /// value      <expr> (               <mode>       of type <typ>)
    ///
    /// commaok    <expr> (<untyped kind> <mode>                    )
    /// commaok    <expr> (               <mode>       of type <typ>)
    pub fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
        tc_objs: &TCObjects,
        ast_objs: &AstObjects,
    ) -> fmt::Result {
        let universe = tc_objs.universe();
        let mut has_expr = true;

        // <expr> (
        if let Some(expr) = &self.expr {
            fmt_expr(&expr, f, ast_objs)?;
        } else {
            match &self.mode {
                OperandMode::Builtin(bi) => {
                    f.write_str(universe.builtins()[bi].name)?;
                }
                OperandMode::TypeExpr => {
                    fmt_type(self.typ, f, tc_objs)?;
                }
                OperandMode::Constant(val) => {
                    f.write_str(&val.to_string())?;
                }
                _ => has_expr = false,
            }
        }
        if has_expr {
            f.write_str(" (")?;
        }

        // <untyped kind>
        let has_type = match self.mode {
            OperandMode::Invalid
            | OperandMode::NoValue
            | OperandMode::Builtin(_)
            | OperandMode::TypeExpr => false,
            _ => {
                let tval = &tc_objs.types[self.typ.unwrap()];
                if tval.is_untyped(tc_objs) {
                    f.write_str(tval.try_as_basic().unwrap().name())?;
                    f.write_char(' ')?;
                    false
                } else {
                    true
                }
            }
        };

        // <mode>
        self.mode.fmt(f)?;

        // <val>
        if let OperandMode::Constant(val) = &self.mode {
            if self.expr.is_some() {
                f.write_char(' ')?;
                f.write_str(&val.to_string())?;
            }
        }

        // <typ>
        if has_type {
            if self.typ != Some(universe.types()[&BasicType::Invalid]) {
                f.write_str(" of type ")?;
                fmt_type(self.typ, f, tc_objs)?;
            } else {
                f.write_str(" with invalid type")?;
            }
        }

        // )
        if has_expr {
            f.write_char(')')?;
        }
        Ok(())
    }
}

/// fmt_expr formats the (possibly shortened) string representation for 'expr'.
/// Shortened representations are suitable for user interfaces but may not
/// necessarily follow Go syntax.
pub fn fmt_expr(expr: &Expr, f: &mut fmt::Formatter<'_>, ast_objs: &AstObjects) -> fmt::Result {
    // The AST preserves source-level parentheses so there is
    // no need to introduce them here to correct for different
    // operator precedences. (This assumes that the AST was
    // generated by a Go parser.)
    ExprFormater {
        f: f,
        ast_objs: ast_objs,
    }
    .visit_expr(expr)
}

struct ExprFormater<'a, 'b> {
    f: &'a mut fmt::Formatter<'b>,
    ast_objs: &'a AstObjects,
}

impl<'a, 'b> ExprVisitor for ExprFormater<'a, 'b> {
    type Result = fmt::Result;

    fn visit_expr(&mut self, expr: &Expr) -> Self::Result {
        walk_expr(self, expr)
    }

    fn visit_expr_ident(&mut self, ident: &IdentKey) -> Self::Result {
        self.f.write_str(&self.ast_objs.idents[*ident].name)
    }

    fn visit_expr_ellipsis(&mut self, els: &Option<Expr>) -> Self::Result {
        self.f.write_str("...")?;
        if let Some(e) = els {
            self.visit_expr(e)?;
        }
        Ok(())
    }

    fn visit_expr_basic_lit(&mut self, blit: &BasicLit) -> Self::Result {
        blit.token.fmt(self.f)
    }

    fn visit_expr_func_lit(&mut self, flit: &FuncLit) -> Self::Result {
        self.f.write_char('(')?;
        self.visit_expr_func_type(&flit.typ)?;
        self.f.write_str(" literal)")
    }

    fn visit_expr_composit_lit(&mut self, clit: &CompositeLit) -> Self::Result {
        self.f.write_char('(')?;
        match &clit.typ {
            Some(t) => {
                self.visit_expr(t)?;
            }
            None => {
                self.f.write_str("(bad expr)")?;
            }
        }
        self.f.write_str(" literal)")
    }

    fn visit_expr_paren(&mut self, expr: &Expr) -> Self::Result {
        self.f.write_char('(')?;
        self.visit_expr(expr)?;
        self.f.write_char(')')
    }

    fn visit_expr_selector(&mut self, expr: &Expr, ident: &IdentKey) -> Self::Result {
        self.visit_expr(expr)?;
        self.f.write_char('.')?;
        self.visit_expr_ident(ident)
    }

    fn visit_expr_index(&mut self, expr: &Expr, index: &Expr) -> Self::Result {
        self.visit_expr(expr)?;
        self.f.write_char('[')?;
        self.visit_expr(index)?;
        self.f.write_char(']')
    }

    fn visit_expr_slice(
        &mut self,
        expr: &Expr,
        low: &Option<Expr>,
        high: &Option<Expr>,
        max: &Option<Expr>,
    ) -> Self::Result {
        self.visit_expr(expr)?;
        self.f.write_char('[')?;
        if let Some(l) = low {
            self.visit_expr(l)?;
        }
        self.f.write_char(':')?;
        if let Some(h) = high {
            self.visit_expr(h)?;
        }
        if let Some(m) = max {
            self.f.write_char(':')?;
            self.visit_expr(m)?;
        }
        self.f.write_char(']')
    }

    fn visit_expr_type_assert(&mut self, expr: &Expr, typ: &Option<Expr>) -> Self::Result {
        self.visit_expr(expr)?;
        self.f.write_str(".(")?;
        match &typ {
            Some(t) => {
                self.visit_expr(t)?;
            }
            None => {
                self.f.write_str("(bad expr)")?;
            }
        }
        self.f.write_char(')')
    }

    fn visit_expr_call(&mut self, func: &Expr, args: &Vec<Expr>, ellipsis: bool) -> Self::Result {
        self.visit_expr(func)?;
        self.f.write_char('(')?;
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.f.write_str(", ")?;
            }
            self.visit_expr(arg)?;
        }
        if ellipsis {
            self.f.write_str("...")?;
        }
        self.f.write_char(')')
    }

    fn visit_expr_star(&mut self, expr: &Expr) -> Self::Result {
        self.f.write_char('*')?;
        self.visit_expr(expr)
    }

    fn visit_expr_unary(&mut self, expr: &Expr, op: &Token) -> Self::Result {
        op.fmt(self.f)?;
        self.visit_expr(expr)
    }

    fn visit_expr_binary(&mut self, left: &Expr, op: &Token, right: &Expr) -> Self::Result {
        self.visit_expr(left)?;
        op.fmt(self.f)?;
        self.visit_expr(right)
    }

    fn visit_expr_key_value(&mut self, _: &Expr, _: &Expr) -> Self::Result {
        self.f.write_str("(bad expr)")
    }

    fn visit_expr_array_type(&mut self, len: &Option<Expr>, elm: &Expr, _: &Expr) -> Self::Result {
        self.f.write_char('[')?;
        if let Some(l) = len {
            self.visit_expr(l)?;
        }
        self.f.write_char(']')?;
        self.visit_expr(elm)
    }

    fn visit_expr_struct_type(&mut self, s: &StructType) -> Self::Result {
        self.f.write_str("struct{")?;
        self.fmt_fields(&s.fields, "; ", false)?;
        self.f.write_char('}')
    }

    fn visit_expr_func_type(&mut self, s: &FuncTypeKey) -> Self::Result {
        self.f.write_str("func")?;
        self.fmt_sig(&self.ast_objs.ftypes[*s])
    }

    fn visit_expr_interface_type(&mut self, s: &InterfaceType) -> Self::Result {
        self.f.write_str("interface{")?;
        self.fmt_fields(&s.methods, "; ", true)?;
        self.f.write_char('}')
    }

    fn visit_map_type(&mut self, key: &Expr, val: &Expr, _: &Expr) -> Self::Result {
        self.f.write_str("map[")?;
        self.visit_expr(key)?;
        self.f.write_char(']')?;
        self.visit_expr(val)
    }

    fn visit_chan_type(&mut self, chan: &Expr, dir: &ChanDir) -> Self::Result {
        let s = match dir {
            ChanDir::Send => "chan<- ",
            ChanDir::Recv => "<-chan ",
            ChanDir::SendRecv => "chan ",
        };
        self.f.write_str(s)?;
        self.visit_expr(chan)
    }

    fn visit_bad_expr(&mut self, _: &BadExpr) -> Self::Result {
        self.f.write_str("(bad expr)")
    }
}

impl<'a, 'b> ExprFormater<'a, 'b> {
    fn fmt_sig(&mut self, sig: &FuncType) -> fmt::Result {
        self.f.write_char('(')?;
        self.fmt_fields(&sig.params, ", ", false)?;
        self.f.write_char(')')?;
        if let Some(re) = &sig.results {
            self.f.write_char(' ')?;
            if re.list.len() == 1 {
                let field = &self.ast_objs.fields[re.list[0]];
                if field.names.len() == 0 {
                    self.visit_expr(&field.typ)?;
                }
            } else {
                self.f.write_char('(')?;
                self.fmt_fields(&re, ", ", false)?;
                self.f.write_char(')')?;
            }
        }
        Ok(())
    }

    fn fmt_fields(&mut self, fields: &FieldList, sep: &str, iface: bool) -> fmt::Result {
        for (i, fkey) in fields.list.iter().enumerate() {
            if i > 0 {
                self.f.write_str(sep)?;
            }
            let field = &self.ast_objs.fields[*fkey];
            for (i, name) in field.names.iter().enumerate() {
                if i > 0 {
                    self.f.write_str(", ")?;
                    self.visit_expr_ident(name)?;
                }
            }
            // types of interface methods consist of signatures only
            if iface {
                match &field.typ {
                    Expr::Func(sig) => {
                        self.fmt_sig(&self.ast_objs.ftypes[*sig])?;
                    }
                    _ => {}
                }
            } else {
                // named fields are separated with a blank from the field type
                if field.names.len() > 0 {
                    self.f.write_char(' ')?;
                }
                self.visit_expr(&field.typ)?;
            }
        }
        Ok(())
    }
}
