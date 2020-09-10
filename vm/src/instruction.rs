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
    LOAD_INDEX,
    STORE_INDEX,
    LOAD_INDEX_IMM,
    STORE_INDEX_IMM,
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
            Opcode::LOAD_INDEX => ("LOAD_INDEX", -1),
            Opcode::STORE_INDEX => ("STORE_INDEX", 0),
            Opcode::LOAD_INDEX_IMM => ("LOAD_INDEX_IMM", 0),
            Opcode::STORE_INDEX_IMM => ("STORE_INDEX_IMM", 0),
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

pub const COPYABLE_END: ValueType = ValueType::Metadata;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Ord, PartialOrd)]
#[repr(u8)]
pub enum ValueType {
    Bool = 1,
    Int,
    Float64,
    Complex64,
    Function,
    Package,
    Metadata,

    // Nil is a virtual type representing zero value for boxed, interfaces,
    // maps, slices, channels and function types
    Nil,
    Boxed,
    Closure,
    Slice,
    Map,
    Interface,
    Channel,

    Str,
    Struct,
}

/// Instruction is 64 bit
/// |    8bit   |    8bit   |    8bit   |    8bit   |    32bit     |
/// |  Opcode   |  <TypeA>  |  <TypeB>  |  <TypeC>  |   immediate  |
/// or
/// |    8bit   |    8bit   |    8bit   |    8bit   |    8bit      |    24bit     |
/// |  Opcode   |  <TypeA>  |  <TypeB>  |  <TypeC>  |     ext      |   immediate  |
/// or
/// |    8bit   |    8bit   |    8bit   |    8bit   |    8bit      |    24bit     |
/// |  Opcode   |  <TypeA>  |  <TypeB>  |    ext    |     ext      |   immediate  |
#[derive(Clone, Copy)]
pub struct Instruction {
    val: u64,
}

impl Instruction {
    pub fn new(
        op: Opcode,
        type0: Option<ValueType>,
        type1: Option<ValueType>,
        type2: Option<ValueType>,
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
        assert!(Instruction::in_8bit_range(imm0));
        assert!(Instruction::in_24bit_range(imm1));
        let u0: u8 = unsafe { std::mem::transmute(imm0 as i8) };
        let u0 = (u0 as u32) << 24;
        let u1: u32 = unsafe { std::mem::transmute(imm1) };
        let u1 = u1 & 0x00ff_ffff;
        self.val = (self.val & 0xffff_ffff_0000_0000) | (u0 | u1) as u64;
    }

    /// set_t2_with_index tries to set an OpIndex to the space of t2
    /// returns error if it's out of range
    /// used by STORE_INDEX_IMM, STORE_FIELD_IMM
    #[inline]
    pub fn set_t2_with_index(&mut self, index: i8) {
        let val8: u8 = unsafe { std::mem::transmute(index) };
        let val64 = (val8 as u64) << 32;
        self.val = (self.val & 0xffff_ff00_ffff_ffff) | val64;
    }

    #[inline]
    pub fn op(&self) -> Opcode {
        unsafe { std::mem::transmute((self.val >> (8 * 3 + 32)) as u8) }
    }

    #[inline]
    pub fn t0(&self) -> ValueType {
        let v = ((self.val >> (8 * 2 + 32)) as u16) & 0xff;
        unsafe { std::mem::transmute(v as u8) }
    }

    #[inline]
    pub fn t1(&self) -> ValueType {
        let v = ((self.val >> (8 + 32)) as u32) & 0xff;
        unsafe { std::mem::transmute(v as u8) }
    }

    #[inline]
    pub fn t2(&self) -> ValueType {
        let v = ((self.val >> 32) as u32) & 0xff;
        unsafe { std::mem::transmute(v as u8) }
    }

    #[inline]
    pub fn t2_as_index(&self) -> OpIndex {
        let v = ((self.val & 0x0000_00ff_0000_0000) >> 32) as u8;
        let ival: i8 = unsafe { std::mem::transmute(v) };
        ival as OpIndex
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

    #[inline]
    pub fn in_8bit_range(i: OpIndex) -> bool {
        -(1 << 7) <= i && i < (1 << 7)
    }

    #[inline]
    pub fn in_24bit_range(i: OpIndex) -> bool {
        -(1 << 23) <= i && i < (1 << 23)
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
            Some(ValueType::Str),
            Some(ValueType::Closure),
            Some(ValueType::Int),
            Some(-99),
        );
        assert_eq!(i.op(), Opcode::ADD);
        assert_eq!(i.t0(), ValueType::Str);
        assert_eq!(i.t1(), ValueType::Closure);
        assert_eq!(i.t2(), ValueType::Int);
        assert_eq!(i.imm(), -99);

        dbg!(1 << 8);
        i.set_imm2(-128, -(1 << 23));
        assert_eq!(i.imm2().0, -128);
        assert_eq!(i.imm2().1, -(1 << 23));
        i.set_imm2(127, 1 << 23 - 1);
        assert_eq!(i.imm2().0, 127);
        assert_eq!(i.imm2().1, 1 << 23 - 1);

        assert!(!Instruction::in_8bit_range(128));
        i.set_t2_with_index(-128);
        assert_eq!(i.t2_as_index(), -128);
        let _ = i.set_t2_with_index(90);
        assert_eq!(i.t2_as_index(), 90);
        assert_eq!(i.t0(), ValueType::Str);
        assert_eq!(i.t1(), ValueType::Closure);
        assert_ne!(i.t2(), ValueType::Int);
        assert_eq!(i.imm2().0, 127);
        assert_eq!(i.imm2().1, 1 << 23 - 1);
    }
}
