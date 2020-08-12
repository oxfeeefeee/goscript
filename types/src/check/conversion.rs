#![allow(dead_code)]
use super::super::constant::Value;
use super::super::objects::TypeKey;
use super::super::operand::{Operand, OperandMode};
use super::super::typ::{self, BasicType, Type};
use super::check::{Checker, FilesContext};
use std::char;

impl<'a> Checker<'a> {
    pub fn conversion(&mut self, x: &mut Operand, t: TypeKey, fctx: &mut FilesContext) {
        let constv = match &mut x.mode {
            OperandMode::Constant(v) => Some(v),
            _ => None,
        };
        let const_arg = constv.is_some();

        let o = &self.tc_objs;
        let xtype = x.typ.unwrap();
        let ok = if const_arg && typ::is_const_type(t, o) {
            // constant conversion
            let v = constv.unwrap();
            let tval = self.otype(t).underlying_val(o);
            let basic = tval.try_as_basic().unwrap();
            let clone = v.clone();
            if clone.representable(basic, Some(v)) {
                true
            } else if typ::is_integer(xtype, o) && tval.is_string(o) {
                let mut s = "\u{FFFD}".to_string();
                let (i, exact) = v.int_as_i64();
                if exact {
                    if let Some(c) = char::from_u32(i as u32) {
                        s = c.to_string()
                    }
                }
                *v = Value::with_str(s);
                true
            } else {
                false
            }
        } else if self.convertable_to(x, t) {
            // non-constant conversion
            x.mode = OperandMode::Value;
            true
        } else {
            false
        };

        if !ok {
            let xd = self.new_dis(x);
            let td = self.new_dis(&t);
            self.error(xd.pos(), format!("cannot convert {} to {}", xd, td));
            x.mode = OperandMode::Invalid;
            return;
        }

        // The conversion argument types are final. For untyped values the
        // conversion provides the type, per the spec: "A constant may be
        // given a type explicitly by a constant declaration or conversion,...".
        if typ::is_untyped(xtype, self.tc_objs) {
            // - For conversions to interfaces, use the argument's default type.
            // - For conversions of untyped constants to non-constant types, also
            //   use the default type (e.g., []byte("foo") should report string
            //   not []byte as type for the constant "foo").
            // - Keep untyped nil for untyped nil arguments.
            // - For integer to string conversions, keep the argument type.
            let final_t = if typ::is_interface(t, o) || const_arg && !typ::is_const_type(t, o) {
                typ::untyped_default_type(xtype, o)
            } else if typ::is_integer(xtype, o) && typ::is_string(t, o) {
                xtype
            } else {
                t
            };
            self.update_expr_type(x.expr.as_ref().unwrap(), final_t, true, fctx);
        }

        x.typ = Some(t);
    }

    // convertible_to returns if x is convertable to t.
    // The check parameter may be nil if convertibleTo is invoked through an
    // exported API call, i.e., when all methods have been type-checked.
    pub fn convertable_to(&self, x: &Operand, t: TypeKey) -> bool {
        let o = &self.tc_objs;
        // "x is assignable to t"
        if x.assignable_to(t, None, o) {
            return true;
        }

        // "x's type and t have identical underlying types if tags are ignored"
        let v = x.typ.unwrap();
        let vu = typ::underlying_type(v, o);
        let tu = typ::underlying_type(t, o);
        if typ::identical_ignore_tags(Some(vu), Some(tu), o) {
            return true;
        }

        let vval = self.otype(v);
        let tval = self.otype(t);
        let vuval = self.otype(vu);
        let tuval = self.otype(tu);
        // "x's type and t are unnamed pointer types and their pointer base types
        // have identical underlying types if tags are ignored"
        if let Some(vdetail) = vval.try_as_pointer() {
            if let Some(tdetail) = tval.try_as_pointer() {
                let vu = typ::underlying_type(vdetail.base(), o);
                let tu = typ::underlying_type(tdetail.base(), o);
                if typ::identical_ignore_tags(Some(vu), Some(tu), o) {
                    return true;
                }
            }
        }

        // "x's type and t are both integer or floating point types"
        if (vval.is_integer(o) || vval.is_float(o)) && (tval.is_integer(o) || tval.is_float(o)) {
            return true;
        }

        // "x's type and t are both complex types"
        if vval.is_complex(o) || tval.is_complex(o) {
            return true;
        }

        // "x is an integer or a slice of bytes or runes and t is a string type"
        if (vval.is_integer(o) || self.is_bytes_or_runes(vuval)) && tval.is_string(o) {
            return true;
        }

        // "x is a string and T is a slice of bytes or runes"
        if vval.is_string(o) && self.is_bytes_or_runes(tuval) {
            return true;
        }

        // package unsafe:
        // "any pointer or value of underlying type uintptr can be converted into a unsafe.Pointer"
        if (self.is_pointer(vuval) || self.is_uintptr(vuval)) && self.is_unsafe_pointer(tval) {
            return true;
        }

        // "and vice versa"
        if (self.is_pointer(tuval) || self.is_uintptr(tuval)) && self.is_unsafe_pointer(vval) {
            return true;
        }

        false
    }

    fn is_uintptr(&self, t: &Type) -> bool {
        if let Some(detail) = t.underlying_val(self.tc_objs).try_as_basic() {
            return detail.typ() == BasicType::Uintptr;
        }
        false
    }

    fn is_unsafe_pointer(&self, t: &Type) -> bool {
        if let Some(detail) = t.underlying_val(self.tc_objs).try_as_basic() {
            return detail.typ() == BasicType::UnsafePointer;
        }
        false
    }

    fn is_pointer(&self, t: &Type) -> bool {
        t.underlying_val(self.tc_objs).try_as_pointer().is_some()
    }

    fn is_bytes_or_runes(&self, t: &Type) -> bool {
        if let Some(detail) = t.try_as_slice() {
            if let Some(b) = self
                .otype(detail.elem())
                .underlying_val(self.tc_objs)
                .try_as_basic()
            {
                return b.typ() == BasicType::Byte || b.typ() == BasicType::Rune;
            }
        }
        false
    }
}
