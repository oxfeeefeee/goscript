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
    LOAD_UPVALUE,
    STORE_UPVALUE,
    LOAD_INDEX,
    STORE_INDEX,
    LOAD_INDEX_IMM,
    STORE_INDEX_IMM,
    LOAD_STRUCT_FIELD,
    STORE_STRUCT_FIELD,
    LOAD_PKG_FIELD,
    STORE_PKG_FIELD,
    LOAD_FIELD,
    STORE_FIELD,
    STORE_DEREF,
    BIND_METHOD,
    BIND_INTERFACE_METHOD,
    CAST,
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
    //REF,       // &
    REF_LOCAL,
    REF_UPVALUE,
    REF_SLICE_MEMBER,
    REF_STRUCT_FIELD,
    REF_PKG_MEMBER,
    REF_LITERAL,
    DEREF, // *
    SEND,  // <-
    RECV,  // <-
    NOT,   // !
    EQL,   // ==
    LSS,   // <
    GTR,   // >
    NEQ,   // !=
    LEQ,   // <=
    GEQ,   // >=

    // call
    PRE_CALL,
    CALL,
    RETURN,

    // jump
    JUMP,
    JUMP_IF,
    JUMP_IF_NOT,
    SWITCH, // EQL + JUMP_IF + do not pop the first argument
    LOOP,
    RANGE_INIT,
    RANGE, // for ... range statement

    // type
    TYPE_ASSERT,
    TYPE,

    // built-in functinalities
    IMPORT,     // imports a package
    SLICE,      // for slice expressions
    SLICE_FULL, // for full slice expressions
    LITERAL,    // for function literal or composite literal
    NEW,        // for built-in function new
    MAKE,       // for built-in function make
    LEN,        // for built-in function len
    CAP,        // for built-in function cap
    APPEND,     // for built-in function append
    ASSERT,     // for built-in function assert
    FFI,        // for built-in function native
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
            Opcode::LOAD_STRUCT_FIELD => ("LOAD_STRUCT_FIELD", 0),
            Opcode::STORE_STRUCT_FIELD => ("STORE_STRUCT_FIELD", 0),
            Opcode::LOAD_PKG_FIELD => ("LOAD_PKG_FIELD", 1),
            Opcode::STORE_PKG_FIELD => ("STORE_PKG_FIELD", 0),
            Opcode::LOAD_FIELD => ("LOAD_FIELD", -1),
            Opcode::STORE_FIELD => ("STORE_FIELD", 0),
            Opcode::STORE_DEREF => ("STORE_DEREF", 0),
            Opcode::BIND_METHOD => ("BIND_METHOD", 0),
            Opcode::BIND_INTERFACE_METHOD => ("BIND_INTERFACE_METHOD", 0),
            Opcode::CAST => ("CAST", 0),

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
            Opcode::REF_LOCAL => ("REF_LOCAL", 0),
            Opcode::REF_UPVALUE => ("REF_UPVALUE", 0),
            Opcode::REF_SLICE_MEMBER => ("REF_SLICE_MEMBER", 0),
            Opcode::REF_STRUCT_FIELD => ("REF_STRUCT_FIELD", 0),
            Opcode::REF_PKG_MEMBER => ("REF_PKG_MEMBER", 0),
            Opcode::REF_LITERAL => ("REF_LITERAL", 0),
            Opcode::DEREF => ("DEREF", 0),
            Opcode::SEND => ("SEND", -1),
            Opcode::RECV => ("RECV", 1),
            Opcode::NOT => ("LNOT", 0),
            Opcode::EQL => ("EQL", -1),
            Opcode::LSS => ("LSS", -1),
            Opcode::GTR => ("GTR", -1),
            Opcode::NEQ => ("NEQ", -1),
            Opcode::LEQ => ("LEQ", -1),
            Opcode::GEQ => ("GEQ", -1),

            Opcode::PRE_CALL => ("PRE_CALL", -128),
            Opcode::CALL => ("CALL", -128),
            Opcode::RETURN => ("RETURN", -128),

            Opcode::JUMP => ("JUMP", 0),
            Opcode::LOOP => ("LOOP", 0),
            Opcode::JUMP_IF => ("JUMP_IF", -1),
            Opcode::JUMP_IF_NOT => ("JUMP_IF_NOT", -1),
            Opcode::SWITCH => ("SWITCH", -1),
            Opcode::RANGE_INIT => ("RANGE_INIT", 0),
            Opcode::RANGE => ("RANGE", 1),

            Opcode::TYPE_ASSERT => ("TYPE_ASSERT", 0),
            Opcode::TYPE => ("TYPE", 1),

            Opcode::IMPORT => ("IMPORT", 0),
            Opcode::SLICE => ("SLICE", -2),
            Opcode::SLICE_FULL => ("SLICE_FULL", -3),
            Opcode::LITERAL => ("LITERAL", 0),
            Opcode::NEW => ("NEW", 0),
            Opcode::MAKE => ("MAKE", 0),
            Opcode::LEN => ("LEN", 0),
            Opcode::CAP => ("CAP", 0),
            Opcode::APPEND => ("APPEND", -128),
            Opcode::ASSERT => ("ASSERT", 0),
            Opcode::FFI => ("FFI", 0),
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

pub const COPYABLE_END: ValueType = ValueType::Package;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Ord, PartialOrd)]
#[repr(u8)]
pub enum ValueType {
    Zero, //place holder
    Bool = 1,
    Int,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    Complex64,
    Function,
    Package,
    Metadata,

    // Nil is a virtual type representing zero value for pointer, interfaces,
    // maps, slices, channels and function types
    Nil,
    Pointer,
    Closure,
    Slice,
    Map,
    Interface,
    Channel,

    Complex128,
    Str,
    Array,
    Struct,

    Named,

    FfiClosure,

    Flag, //not a type, works as a flag in instructions
}

impl ValueType {
    #[inline]
    pub fn copyable(&self) -> bool {
        self <= &COPYABLE_END
    }
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
/// or
/// | package_key|
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
    pub fn from_u64(v: u64) -> Instruction {
        Instruction { val: v }
    }

    #[inline]
    pub fn set_imm(&mut self, imm: OpIndex) {
        let uv: u32 = unsafe { std::mem::transmute(imm) };
        self.val = (self.val & 0xffff_ffff_0000_0000) | uv as u64;
    }

    /// set_imm824 sets an 8bit imm and a 24bit imm at the lower 4 bytes
    #[inline]
    pub fn set_imm824(&mut self, imm0: OpIndex, imm1: OpIndex) {
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
    /// used by STORE_INDEX_IMM, STORE_STRUCT_FIELD
    #[inline]
    pub fn set_t2_with_index(&mut self, index: i8) {
        let val8: u8 = unsafe { std::mem::transmute(index) };
        let val64 = (val8 as u64) << 32;
        self.val = (self.val & 0xffff_ff00_ffff_ffff) | val64;
    }

    #[inline]
    pub fn get_u64(&self) -> u64 {
        self.val
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
    pub fn imm824(&self) -> (OpIndex, OpIndex) {
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
            | Opcode::STORE_STRUCT_FIELD
            | Opcode::STORE_PKG_FIELD
            | Opcode::STORE_DEREF => {
                let (i0, i1) = self.imm824();
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
        i.set_imm824(-128, -(1 << 23));
        assert_eq!(i.imm824().0, -128);
        assert_eq!(i.imm824().1, -(1 << 23));
        i.set_imm824(127, 1 << 23 - 1);
        assert_eq!(i.imm824().0, 127);
        assert_eq!(i.imm824().1, 1 << 23 - 1);

        assert!(!Instruction::in_8bit_range(128));
        i.set_t2_with_index(-128);
        assert_eq!(i.t2_as_index(), -128);
        let _ = i.set_t2_with_index(90);
        assert_eq!(i.t2_as_index(), 90);
        assert_eq!(i.t0(), ValueType::Str);
        assert_eq!(i.t1(), ValueType::Closure);
        assert_ne!(i.t2(), ValueType::Int);
        assert_eq!(i.imm824().0, 127);
        assert_eq!(i.imm824().1, 1 << 23 - 1);
    }
}
