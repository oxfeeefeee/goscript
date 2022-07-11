// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(non_camel_case_types)]
use std::fmt::Debug;
use std::fmt::{self};

pub type OpIndex = i32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    VOID,

    DUPLICATE,
    LOAD_SLICE,
    STORE_SLICE,
    LOAD_ARRAY,
    STORE_ARRAY,
    LOAD_MAP,
    STORE_MAP,
    LOAD_STRUCT,
    STORE_STRUCT,
    LOAD_EMBEDDED,
    STORE_EMBEDDED,
    LOAD_PKG,
    STORE_PKG,
    LOAD_POINTER,
    STORE_POINTER,
    LOAD_UP_VALUE,
    STORE_UP_VALUE,

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
    INC,            //++
    DEC,            //--
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
    REF_EMBEDDED,
    REF_PKG_MEMBER,
    SEND, // <-
    RECV, // <-

    // call
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
    LOAD_INIT_FUNC,
    BIND_METHOD,
    BIND_I_METHOD,
    CAST,
    TYPE_ASSERT,
    TYPE,

    // built-in functinalities
    IMPORT,  // imports a package
    SLICE,   // for slice expressions
    CLOSURE, // for creating a closure with function literal
    LITERAL, // for composite literal
    NEW,     // for built-in function new
    MAKE,    // for built-in function make
    COMPLEX, // for built-in function complex
    REAL,    // for built-in function real
    IMAG,    // for built-in function imag
    LEN,     // for built-in function len
    CAP,     // for built-in function cap
    APPEND,  // for built-in function append
    DELETE,  // for built-in function delete
    COPY,    // for built-in function copy
    CLOSE,   // for built-in function close
    PANIC,   // for built-in function panic
    RECOVER, // for built-in function recover
    ASSERT,  // for built-in function assert
    FFI,     // for built-in function native
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub const COPYABLE_END: ValueType = ValueType::Package;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
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

#[derive(Clone, Copy)]
pub struct Instruction {
    pub op0: Opcode,
    pub op1: Opcode,
    pub t0: ValueType,
    pub t1: ValueType,
    pub d: OpIndex,
    pub s0: OpIndex,
    pub s1: OpIndex,
}

impl Instruction {
    pub fn op1_as_t(&self) -> ValueType {
        unsafe { std::mem::transmute(self.op1) }
    }
}

impl std::fmt::Debug for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let ops = if self.op1 == Opcode::VOID {
            self.op0.to_string()
        } else {
            format!("{}.{}", self.op0, self.op1)
        };
        write!(f, "{: <16}|", ops)?;
        fmt_index(self.d, f)?;
        f.write_str("\t|")?;
        fmt_index(self.s0, f)?;
        f.write_str("\t|")?;
        fmt_index(self.s1, f)?;
        f.write_str("\t|")?;
        fmt_type(self.t0, f)?;
        f.write_str("\t|")?;
        fmt_type(self.t1, f)?;
        Ok(())
    }
}

fn fmt_type(t: ValueType, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    if t == ValueType::Void {
        f.write_str("...")
    } else {
        t.fmt(f)
    }
}

fn fmt_index(index: OpIndex, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    if index == OpIndex::MAX {
        f.write_str("...")
    } else {
        index.fmt(f)
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
