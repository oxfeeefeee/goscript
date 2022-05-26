// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(non_camel_case_types)]
use std::fmt;

pub type OpIndex = i32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    VOID,
    LOAD_SLICE,
    STORE_SLICE,
    LOAD_ARRAY,
    STORE_ARRAY,
    LOAD_MAP,
    STORE_MAP,
    LOAD_STRUCT,
    STORE_STRUCT,
    LOAD_STRUCT_EMBEDDED,
    STORE_STRUCT_EMBEDDED,
    LOAD_PKG,
    STORE_PKG,
    LOAD_POINTER,
    STORE_POINTER,
    LOAD_UPVALUE,
    STORE_UPVALUE,

    // arithmetic, logical, ref, arrow
    ADD,            // +
    SUB,            // -
    MUL,            // *
    QUO,            // /
    REM,            // %
    AND,            // &
    OR,             // |
    XOR,            // ^
    AND_NOT,        // $^
    SHL,            // <<
    SHR,            // >>
    ADD_ASSIGN,     // +
    SUB_ASSIGN,     // -
    MUL_ASSIGN,     // *
    QUO_ASSIGN,     // /
    REM_ASSIGN,     // %
    AND_ASSIGN,     // &
    OR_ASSIGN,      // |
    XOR_ASSIGN,     // ^
    AND_NOT_ASSIGN, // $^
    SHL_ASSIGN,     // <<
    SHR_ASSIGN,     // >>
    UNARY_ADD,      // +
    UNARY_SUB,      // -
    UNARY_XOR,      // ^
    NOT,            // !
    EQL,            // ==
    NEQ,            // !=
    LSS,            // <
    GTR,            // >
    LEQ,            // <=
    GEQ,            // >=
    REF,            // &
    REF_UPVALUE,
    REF_SLICE_MEMBER,
    REF_STRUCT_FIELD,
    REF_STRUCT_EMBEDDED_FIELD,
    REF_PKG_MEMBER,
    SEND, // <-
    RECV, // <-

    // call
    PRE_CALL,
    PACK_VARIADIC,
    CALL,
    RETURN,

    // jump
    JUMP,
    JUMP_IF,
    JUMP_IF_NOT,
    SWITCH,
    SELECT,
    RANGE_INIT,
    RANGE,

    // misc
    LOAD_PKG_INIT_FUNC,
    BIND_METHOD,
    BIND_INTERFACE_METHOD,
    CAST,
    TYPE_ASSERT,
    TYPE,

    // built-in functinalities
    IMPORT,     // imports a package
    SLICE,      // for slice expressions
    SLICE_FULL, // for full slice expressions
    LITERAL,    // for function literal or composite literal
    NEW,        // for built-in function new
    MAKE,       // for built-in function make
    COMPLEX,    // for built-in function complex
    REAL,       // for built-in function real
    IMAG,       // for built-in function imag
    LEN,        // for built-in function len
    CAP,        // for built-in function cap
    APPEND,     // for built-in function append
    DELETE,     // for built-in function delete
    COPY,       // for built-in function copy
    CLOSE,      // for built-in function close
    PANIC,      // for built-in function panic
    RECOVER,    // for built-in function recover
    ASSERT,     // for built-in function assert
    FFI,        // for built-in function native
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub const COPYABLE_END: ValueType = ValueType::Package;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Ord, PartialOrd)]
#[repr(u8)]
pub enum ValueType {
    Void,
    Bool,
    Int,
    Int8,
    Int16,
    Int32,
    Int64,
    Uint,
    UintPtr,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    Complex64,
    Function,
    Package, //COPYABLE_END
    Metadata,
    Complex128,
    String,
    Array,
    Struct,
    Pointer,
    UnsafePtr,
    Closure,
    Slice,
    Map,
    Interface,
    Channel,

    FlagA, //not a type, works as a flag in instructions
    FlagB,
    FlagC,
    FlagD,
    FlagE,
}

impl ValueType {
    #[inline]
    pub fn copyable(&self) -> bool {
        self <= &COPYABLE_END
    }
}

#[derive(Clone, Debug)]
pub struct Instruction {
    pub op0: Opcode,
    pub op1: Opcode,
    pub t0: ValueType,
    pub t1: ValueType,
    pub d: OpIndex,
    pub s0: OpIndex,
    pub s1: OpIndex,
}

impl Default for Instruction {
    fn default() -> Self {
        Instruction {
            op0: Opcode::VOID,
            op1: Opcode::VOID,
            t0: ValueType::Void,
            t1: ValueType::Void,
            d: 0,
            s0: 0,
            s1: 0,
        }
    }
}

impl Instruction {
    pub fn with_op_t_index(
        op: Opcode,
        t0: Option<ValueType>,
        t1: Option<ValueType>,
        d: OpIndex,
        s0: OpIndex,
        s1: OpIndex,
    ) -> Self {
        Self {
            op0: op,
            op1: Opcode::VOID,
            t0: t0.unwrap_or(ValueType::Void),
            t1: t1.unwrap_or(ValueType::Void),
            d: d,
            s0: s0,
            s1: s1,
        }
    }

    pub fn with_op_index(op: Opcode, d: OpIndex, s0: OpIndex, s1: OpIndex) -> Self {
        Self::with_op_t_index(op, None, None, d, s0, s1)
    }

    pub fn set_op1_with_t(&mut self, t: ValueType) {
        self.op1 = unsafe { std::mem::transmute(t) }
    }

    pub fn op1_as_t(&self) -> ValueType {
        unsafe { std::mem::transmute(self.op1) }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_inst_size() {
        println!("size {} \n", std::mem::size_of::<Instruction>());
    }
}
