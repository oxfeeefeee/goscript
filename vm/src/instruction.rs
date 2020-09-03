#![allow(dead_code)]
#![allow(non_camel_case_types)]
use std::fmt;

pub type OpIndex = i32;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Opcode {
    ZERO = 0, //place holder
    // push pop load store
    PUSH_CONST,
    PUSH_NIL,
    PUSH_FALSE,
    PUSH_TRUE,
    PUSH_IMM,
    POP,
    LOAD_LOCAL,
    STORE_LOCAL, // stores the value on the top of the stack to local
    // STORE_INTERFACE
    LOAD_UPVALUE,
    STORE_UPVALUE,
    LOAD_FIELD,
    STORE_FIELD,
    LOAD_FIELD_IMM,
    STORE_FIELD_IMM,
    //LOAD_FIELD_BY_NAME,
    //STORE_FIELD_BY_NAME,
    LOAD_THIS_PKG_FIELD,
    STORE_THIS_PKG_FIELD,
    STORE_DEREF,
    // arithmetic, logical, ref, deref, arrow
    ADD,       // +
    SUB,       // -
    MUL,       // *
    QUO,       // /
    REM,       // %
    AND,       // &
    OR,        // |
    XOR,       // ^
    SHL,       // <<
    SHR,       // >>
    AND_NOT,   // $^
    UNARY_ADD, // +
    UNARY_SUB, // -
    UNARY_XOR, // ^
    REF,       // &
    DEREF,     // *
    ARROW,     // <-
    NOT,       // !
    EQL,       // ==
    LSS,       // <
    GTR,       // >
    NEQ,       // !=
    LEQ,       // <=
    GEQ,       // >=

    // call
    PRE_CALL,
    CALL,
    CALL_ELLIPSIS, // call with the past parameter followed by ellipsis
    CLOSE_UPVALUE,
    RETURN,
    RETURN_INIT_PKG,

    // jump
    JUMP,
    JUMP_IF,
    JUMP_IF_NOT,
    LOOP,
    RANGE, // for ... range statement

    // built-in functinalities
    IMPORT,     // imports a package
    SLICE,      //for slice expressions
    SLICE_FULL, // for full slice expressions
    NEW,        // for built-in function new
    MAKE,       // for built-in function make
    LEN,        // for built-in function len
    CAP,        // for built-in function cap
    APPEND,     //for built-in function append
    ASSERT,     //for built-in function assert
}

impl Opcode {
    #[inline]
    pub fn offset(&self, base: Opcode) -> OpIndex {
        (*self as i16 - base as i16) as OpIndex
    }

    pub fn property(&self) -> (&str, i8) {
        match self {
            Opcode::ZERO => ("ZERO (place holder)", 0),
            Opcode::PUSH_CONST => ("PUSH_CONST", 1),
            Opcode::PUSH_NIL => ("PUSH_NIL", 1),
            Opcode::PUSH_FALSE => ("PUSH_FALSE", 1),
            Opcode::PUSH_TRUE => ("PUSH_TRUE", 1),
            Opcode::PUSH_IMM => ("PUSH_IMM", 1),
            Opcode::POP => ("POP", -1),
            Opcode::LOAD_LOCAL => ("LOAD_LOCAL", 1),
            Opcode::STORE_LOCAL => ("STORE_LOCAL", 0),
            Opcode::LOAD_UPVALUE => ("LOAD_LOCAL", 1),
            Opcode::STORE_UPVALUE => ("STORE_UPVALUE", 0),
            Opcode::LOAD_FIELD => ("LOAD_FIELD", -1),
            Opcode::STORE_FIELD => ("STORE_FIELD", 0),
            Opcode::LOAD_FIELD_IMM => ("LOAD_FIELD_IMM", 0),
            Opcode::STORE_FIELD_IMM => ("STORE_FIELD_IMM", 0),
            Opcode::LOAD_THIS_PKG_FIELD => ("LOAD_THIS_PKG_FIELD", -1),
            Opcode::STORE_THIS_PKG_FIELD => ("STORE_THIS_PKG_FIELD", 0),
            Opcode::STORE_DEREF => ("STORE_DEREF", 0),

            Opcode::ADD => ("ADD", -1),
            Opcode::SUB => ("SUB", -1),
            Opcode::MUL => ("MUL", -1),
            Opcode::QUO => ("QUO", -1),
            Opcode::REM => ("REM", -1),
            Opcode::AND => ("AND", -1),
            Opcode::OR => ("OR", -1),
            Opcode::XOR => ("XOR", -1),
            Opcode::SHL => ("SHL", -1),
            Opcode::SHR => ("SHR", -1),
            Opcode::AND_NOT => ("AND_NOT", -1),
            Opcode::UNARY_ADD => ("UNARY_ADD", 0),
            Opcode::UNARY_SUB => ("UNARY_SUB", 0),
            Opcode::UNARY_XOR => ("UNARY_XOR", 0),
            Opcode::REF => ("REF", 0),
            Opcode::DEREF => ("DEREF", 0),
            Opcode::ARROW => ("ARROW", 0),
            Opcode::NOT => ("LNOT", 0),
            Opcode::EQL => ("EQL", -1),
            Opcode::LSS => ("LSS", -1),
            Opcode::GTR => ("GTR", -1),
            Opcode::NEQ => ("NEQ", -1),
            Opcode::LEQ => ("LEQ", -1),
            Opcode::GEQ => ("GEQ", -1),

            Opcode::PRE_CALL => ("PRE_CALL", -128),
            Opcode::CALL => ("CALL", -128),
            Opcode::CALL_ELLIPSIS => ("CALL_ELLIPSIS", -128),
            Opcode::CLOSE_UPVALUE => ("CLOSE_UPVALUE", -1),
            Opcode::RETURN => ("RETURN", -128),
            Opcode::RETURN_INIT_PKG => ("RETURN_INIT_PKG", -128),

            Opcode::JUMP => ("JUMP", 0),
            Opcode::LOOP => ("LOOP", 0),
            Opcode::JUMP_IF => ("JUMP_IF", -1),
            Opcode::JUMP_IF_NOT => ("JUMP_IF_NOT", -1),
            Opcode::RANGE => ("RANGE", 1),

            Opcode::IMPORT => ("IMPORT", 0),
            Opcode::SLICE => ("SLICE", -2),
            Opcode::SLICE_FULL => ("SLICE_FULL", -3),
            Opcode::NEW => ("NEW", 0),
            Opcode::MAKE => ("MAKE", 0),
            Opcode::LEN => ("LEN", 0),
            Opcode::CAP => ("CAP", 0),
            Opcode::APPEND => ("APPEND", -128),
            Opcode::ASSERT => ("ASSERT", 0),
        }
    }

    pub fn text(&self) -> &str {
        let (t, _) = self.property();
        t
    }
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (t, _) = self.property();
        write!(f, "OPCODE: {}", t)
    }
}

pub const COPYABLE_END: Value32Type = Value32Type::Metadata;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Ord, PartialOrd)]
#[repr(u8)]
pub enum Value32Type {
    Copyable = 1, // a virtual type indicating any non-rc type
    Nil,
    Bool,
    Int,
    Float64,
    Complex64,
    Function,
    Package,
    Metadata,

    Str,
    Boxed,
    Closure,
    Slice,
    Map,
    Interface,
    Struct,
    Channel,
}

/// Instruction is 64 bit
/// |    8bit   |    8bit   |    8bit   |    8bit   |    32bit     |
/// |  Opcode   |  <TypeA>  |  <TypeB>  |  <TypeC>  |   immediate  |
/// or
/// |    8bit   |    8bit   |    8bit   |    8bit   |    8bit      |    24bit     |
/// |  Opcode   |  <TypeA>  |  <TypeB>  |  <TypeC>  |     ext      |   immediate  |
#[derive(Clone, Copy)]
pub struct Instruction {
    val: u64,
}

impl Instruction {
    pub fn new(
        op: Opcode,
        type0: Option<Value32Type>,
        type1: Option<Value32Type>,
        type2: Option<Value32Type>,
        imm: Option<OpIndex>,
    ) -> Instruction {
        let val = (op as u64) << (8 * 3 + 32);
        let mut inst = Instruction { val: val };
        if let Some(v) = type0 {
            inst.val |= (v as u64) << (8 * 2 + 32);
        }
        if let Some(v) = type1 {
            inst.val |= (v as u64) << (8 + 32);
        }
        if let Some(v) = type2 {
            inst.val |= (v as u64) << 32;
        }
        if let Some(v) = imm {
            inst.set_imm(v);
        }
        inst
    }

    #[inline]
    pub fn set_imm(&mut self, imm: OpIndex) {
        let uv: u32 = unsafe { std::mem::transmute(imm) };
        self.val = (self.val & 0xffff_ffff_0000_0000) | uv as u64;
    }

    #[inline]
    pub fn set_imm2(&mut self, imm0: OpIndex, imm1: OpIndex) {
        assert!(-(1 << 7) <= imm0 && imm0 < (1 << 7));
        assert!(-(1 << 23) <= imm1 && imm1 < (1 << 23));
        let u0: u8 = unsafe { std::mem::transmute(imm0 as i8) };
        let u0 = (u0 as u32) << 24;
        let u1: u32 = unsafe { std::mem::transmute(imm1) };
        let u1 = u1 & 0x00ff_ffff;
        self.val = (self.val & 0xffff_ffff_0000_0000) | (u0 | u1) as u64;
    }

    #[inline]
    pub fn op(&self) -> Opcode {
        unsafe { std::mem::transmute((self.val >> (8 * 3 + 32)) as u8) }
    }

    #[inline]
    pub fn t0(&self) -> Value32Type {
        let v = ((self.val >> (8 * 2 + 32)) as u16) & 0xff;
        unsafe { std::mem::transmute(v as u8) }
    }

    #[inline]
    pub fn t1(&self) -> Value32Type {
        let v = ((self.val >> (8 + 32)) as u32) & 0xff;
        unsafe { std::mem::transmute(v as u8) }
    }

    #[inline]
    pub fn t2(&self) -> Value32Type {
        let v = ((self.val >> 32) as u32) & 0xff;
        unsafe { std::mem::transmute(v as u8) }
    }

    #[inline]
    pub fn imm(&self) -> OpIndex {
        unsafe { std::mem::transmute((self.val & 0xffff_ffff) as u32) }
    }

    #[inline]
    pub fn imm2(&self) -> (OpIndex, OpIndex) {
        let all = (self.val & 0xffff_ffff) as u32;
        let i0: i8 = unsafe { std::mem::transmute((all >> 24) as u8) };
        let all = all & 0x00ff_ffff;
        let sign = all >> 23;
        let all = ((sign * 0xff) << 24) | all;
        let i1 = unsafe { std::mem::transmute(all) };
        (i0 as OpIndex, i1)
    }

    #[inline]
    pub fn code2index(op: Opcode) -> OpIndex {
        op as OpIndex
    }

    #[inline]
    pub fn index2code(i: OpIndex) -> Opcode {
        unsafe { std::mem::transmute(i as u8) }
    }
}

impl fmt::Debug for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let op = self.op();
        match op {
            Opcode::STORE_LOCAL
            | Opcode::STORE_UPVALUE
            | Opcode::STORE_FIELD
            | Opcode::STORE_FIELD_IMM
            | Opcode::STORE_THIS_PKG_FIELD
            | Opcode::STORE_DEREF => {
                let (i0, i1) = self.imm2();
                if i0 < 0 {
                    write!(f, "{}, IMM0: {}, IMM1: {}", op, i0, i1)
                } else {
                    let op_ex = Instruction::index2code(i0);
                    write!(f, "{}, EX: {}, IMM0: {}, IMM1: {}", op, op_ex, i0, i1)
                }
            }
            _ => {
                let imm = self.imm();
                write!(f, "{}, IMM: {}", op, imm)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_opcode() {
        println!("opcode {} \n", Opcode::POP);
    }

    #[test]
    fn test_instruction() {
        let mut i = Instruction::new(
            Opcode::ADD,
            Some(Value32Type::Str),
            Some(Value32Type::Closure),
            Some(Value32Type::Int),
            Some(-99),
        );
        assert_eq!(i.op(), Opcode::ADD);
        assert_eq!(i.t0(), Value32Type::Str);
        assert_eq!(i.t1(), Value32Type::Closure);
        assert_eq!(i.t2(), Value32Type::Int);
        assert_eq!(i.imm(), -99);

        dbg!(1 << 8);
        i.set_imm2(-128, -(1 << 23));
        assert_eq!(i.imm2().0, -128);
        assert_eq!(i.imm2().1, -(1 << 23));
        i.set_imm2(127, 1 << 23 - 1);
        assert_eq!(i.imm2().0, 127);
        assert_eq!(i.imm2().1, 1 << 23 - 1);
    }
}
