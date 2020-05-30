#![allow(dead_code)]
use super::constant;
use super::lookup::missing_method;
use super::objects::{TCObjects, TypeKey};
use super::typ;
use super::typ::{BasicType, Type};
use super::universe::{Builtin, Universe};
use goscript_parser::ast;
use goscript_parser::ast::Node;
use goscript_parser::objects::Objects as AstObject;
use goscript_parser::position;
use goscript_parser::token::Token;
use std::fmt;

/// An OperandMode specifies the (addressing) mode of an operand.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum OperandMode {
    Invalid,  // operand is invalid
    NoValue,  // operand represents no value (result of a function call w/o result)
    Builtin,  // operand is a built-in function
    TypeExpr, // operand is a type
    Constant, // operand is a constant; the operand's typ is a Basic type
    Variable, // operand is an addressable variable
    MapIndex, // operand is a map index expression (acts like a variable on lhs, commaok on rhs of an assignment)
    Value,    // operand is a computed value
    CommaOk,  // like value, but operand may be used in a comma,ok expression
}

impl fmt::Display for OperandMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            OperandMode::Invalid => "invalid operand",
            OperandMode::NoValue => "no value",
            OperandMode::Builtin => "built-in",
            OperandMode::TypeExpr => "type",
            OperandMode::Constant => "constant",
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
/// The zero value of operand is a ready to use invalid operand.
pub struct Operand {
    mode: OperandMode,
    expr: Option<ast::Expr>,
    typ: TypeKey,
    val: constant::Value,
    builtin: Builtin,
}

impl Operand {
    pub fn pos(&self, ast_objs: &AstObject) -> position::Pos {
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
        self.mode = OperandMode::Constant;
        self.typ = u.types()[&bt];
        self.val = constant::Value::with_literal(t);
    }

    pub fn is_nil(&self, u: &Universe) -> bool {
        self.mode == OperandMode::Value && self.typ == u.types()[&BasicType::UntypedNil]
    }

    /// assignable_to reports whether self is assignable to a variable of type 't'.
    /// If the result is false and a non-None reason is provided, it may be set
    /// to a more detailed explanation of the failure.
    pub fn assignable_to(
        &self,
        t: &TypeKey,
        reason: Option<&mut String>,
        objs: &TCObjects,
    ) -> bool {
        let u = objs.universe();
        if self.mode == OperandMode::Invalid || *t == u.types()[&BasicType::Invalid] {
            return true; // avoid spurious errors
        }
        if typ::identical(&self.typ, t, objs) {
            return true;
        }
        let t_left = &objs.types[*t];
        let t_right = &objs.types[self.typ];
        let ut_key_left = typ::underlying_type(t, objs);
        let ut_key_right = typ::underlying_type(&self.typ, objs);
        let ut_left = &objs.types[*ut_key_left];
        let ut_right = &objs.types[*ut_key_right];

        if ut_right.is_untyped(objs) {
            match ut_left {
                Type::Basic(detail) => {
                    if self.is_nil(u) && detail.typ() == BasicType::UnsafePointer {
                        return true;
                    }
                    if self.mode == OperandMode::Constant {
                        return self.val.representable(detail.typ(), None);
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
            if let Some((m, wrong_type)) = missing_method(ut_key_right, ut_key_left, true, objs) {
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
            if *cr.dir() == typ::ChanDir::SendRecv {
                if let Some(cl) = ut_left.try_as_chan() {
                    if typ::identical(cr.elem(), cl.elem(), objs) {
                        return !t_right.is_named() || !t_left.is_named();
                    }
                }
            }
        }

        false
    }
}
