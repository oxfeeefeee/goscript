// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

#![allow(non_camel_case_types)]
use borsh::{
    maybestd::io::Result as BorshResult, maybestd::io::Write as BorshWrite, BorshDeserialize,
    BorshSerialize,
};
use std::fmt;
use std::fmt::Debug;

pub type OpIndex = i32;

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    COPY,    // for built-in function copy
    DELETE,  // for built-in function delete
    CLOSE,   // for built-in function close
    PANIC,   // for built-in function panic
    RECOVER, // for built-in function recover
    ASSERT,  // for built-in function assert
    FFI,     // for FFI
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(
    BorshDeserialize, BorshSerialize, Copy, Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd,
)]
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
    Pointer, // COMPARABLE_END
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
        self <= &Self::Package
    }

    #[inline]
    pub fn comparable(&self) -> bool {
        self <= &Self::Pointer
    }

    #[inline]
    pub fn nilable(&self) -> bool {
        self >= &Self::Pointer && self <= &Self::Channel
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

    // Get the max register index 'instructions' write to
    pub fn max_write_index(instructions: &[Instruction]) -> OpIndex {
        let mut i = 0;
        let mut result = 0;
        loop {
            let cur = &instructions[i];
            let index = match cur.op0 {
                Opcode::VOID => 0,
                Opcode::DUPLICATE => cur.d,
                Opcode::LOAD_SLICE => cur.d,
                Opcode::STORE_SLICE => 0,
                Opcode::LOAD_ARRAY => cur.d,
                Opcode::STORE_ARRAY => 0,
                Opcode::LOAD_MAP => {
                    i += 1;
                    match cur.t1 {
                        ValueType::FlagB => instructions[i].d,
                        _ => cur.d,
                    }
                }
                Opcode::STORE_MAP => {
                    i += 1;
                    0
                }
                Opcode::LOAD_STRUCT => cur.d,
                Opcode::STORE_STRUCT => 0,
                Opcode::LOAD_EMBEDDED => cur.d,
                Opcode::STORE_EMBEDDED => 0,
                Opcode::LOAD_PKG => cur.d,
                Opcode::STORE_PKG => 0,
                Opcode::LOAD_POINTER => cur.d,
                Opcode::STORE_POINTER => 0,
                Opcode::LOAD_UP_VALUE => cur.d,
                Opcode::STORE_UP_VALUE => 0,
                Opcode::ADD => cur.d,
                Opcode::SUB => cur.d,
                Opcode::MUL => cur.d,
                Opcode::QUO => cur.d,
                Opcode::REM => cur.d,
                Opcode::AND => cur.d,
                Opcode::OR => cur.d,
                Opcode::XOR => cur.d,
                Opcode::AND_NOT => cur.d,
                Opcode::SHL => cur.d,
                Opcode::SHR => cur.d,
                Opcode::ADD_ASSIGN => 0,
                Opcode::SUB_ASSIGN => 0,
                Opcode::MUL_ASSIGN => 0,
                Opcode::QUO_ASSIGN => 0,
                Opcode::REM_ASSIGN => 0,
                Opcode::AND_ASSIGN => 0,
                Opcode::OR_ASSIGN => 0,
                Opcode::XOR_ASSIGN => 0,
                Opcode::AND_NOT_ASSIGN => 0,
                Opcode::SHL_ASSIGN => 0,
                Opcode::SHR_ASSIGN => 0,
                Opcode::INC => 0,
                Opcode::DEC => 0,
                Opcode::UNARY_SUB => cur.d,
                Opcode::UNARY_XOR => cur.d,
                Opcode::NOT => cur.d,
                Opcode::EQL => cur.d,
                Opcode::NEQ => cur.d,
                Opcode::LSS => cur.d,
                Opcode::GTR => cur.d,
                Opcode::LEQ => cur.d,
                Opcode::GEQ => cur.d,
                Opcode::REF => cur.d,
                Opcode::REF_UPVALUE => cur.d,
                Opcode::REF_SLICE_MEMBER => cur.d,
                Opcode::REF_STRUCT_FIELD => cur.d,
                Opcode::REF_EMBEDDED => cur.d,
                Opcode::REF_PKG_MEMBER => cur.d,
                Opcode::SEND => 0,
                Opcode::RECV => match cur.t1 {
                    ValueType::FlagB => std::cmp::max(cur.d, cur.s1),
                    _ => cur.d,
                },
                Opcode::PACK_VARIADIC => cur.d,
                Opcode::CALL => 0,
                Opcode::RETURN => 0,
                Opcode::JUMP => 0,
                Opcode::JUMP_IF => 0,
                Opcode::JUMP_IF_NOT => 0,
                Opcode::SWITCH => 0,
                Opcode::SELECT => {
                    let begin = i + 1;
                    i += cur.s0 as usize;
                    instructions[begin..i].iter().fold(0, |acc, x| {
                        let val = match x.t0 {
                            ValueType::FlagC => x.s1,
                            ValueType::FlagD => x.s1 + 1,
                            _ => 0,
                        };
                        std::cmp::max(acc, val)
                    })
                }
                Opcode::RANGE_INIT => 0,
                Opcode::RANGE => 0,
                Opcode::LOAD_INIT_FUNC => {
                    i += 2;
                    std::cmp::max(cur.d, cur.s1)
                }
                Opcode::BIND_METHOD => cur.d,
                Opcode::BIND_I_METHOD => cur.d,
                Opcode::CAST => cur.d,
                Opcode::TYPE_ASSERT => match cur.t1 {
                    ValueType::FlagB => {
                        i += 1;
                        instructions[i].d
                    }
                    _ => cur.d,
                },
                Opcode::TYPE => match cur.t0 {
                    ValueType::FlagA => std::cmp::max(cur.d, cur.s1),
                    _ => cur.d,
                },
                Opcode::IMPORT => 0,
                Opcode::SLICE => {
                    i += 1;
                    cur.d
                }
                Opcode::CLOSURE => cur.d,
                Opcode::LITERAL => {
                    i += 1 + cur.s1 as usize;
                    cur.d
                }
                Opcode::NEW => cur.d,
                Opcode::MAKE => {
                    if cur.t0 == ValueType::FlagC {
                        i += 1;
                    }
                    cur.d
                }
                Opcode::COMPLEX => cur.d,
                Opcode::REAL => cur.d,
                Opcode::IMAG => cur.d,
                Opcode::LEN => cur.d,
                Opcode::CAP => cur.d,
                Opcode::APPEND => cur.d,
                Opcode::COPY => cur.d,
                Opcode::DELETE => 0,
                Opcode::CLOSE => 0,
                Opcode::PANIC => 0,
                Opcode::RECOVER => cur.d,
                Opcode::ASSERT => 0,
                Opcode::FFI => cur.d,
            };
            result = std::cmp::max(result, index);
            i += 1;
            if i >= instructions.len() {
                break;
            }
        }
        result
    }
}

impl BorshSerialize for Instruction {
    fn serialize<W: BorshWrite>(&self, writer: &mut W) -> BorshResult<()> {
        self.op0.serialize(writer)?;
        self.op1.serialize(writer)?;
        self.t0.serialize(writer)?;
        self.t1.serialize(writer)?;
        self.d.serialize(writer)?;
        self.s0.serialize(writer)?;
        self.s1.serialize(writer)
    }
}

impl BorshDeserialize for Instruction {
    fn deserialize(buf: &mut &[u8]) -> BorshResult<Self> {
        let op0 = Opcode::deserialize(buf)?;
        let op1 = Opcode::deserialize(buf)?;
        let t0 = ValueType::deserialize(buf)?;
        let t1 = ValueType::deserialize(buf)?;
        let d = OpIndex::deserialize(buf)?;
        let s0 = OpIndex::deserialize(buf)?;
        let s1 = OpIndex::deserialize(buf)?;
        Ok(Instruction {
            op0,
            op1,
            t0,
            t1,
            d,
            s0,
            s1,
        })
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
