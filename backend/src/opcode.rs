#![allow(dead_code)]
#![allow(non_camel_case_types)]
use std::fmt;

pub const MAX_INLINE_LOCAL_INDEX: OpIndex = 15;
pub const MAX_INLINE_ARG_INDEX: OpIndex = 15;

pub type OpIndex = i16;

#[derive(Clone, Copy, Debug)]
pub enum Opcode {
    PUSH_CONST = 100,
    PUSH_NIL = 101,
    PUSH_FALSE = 102,
    PUSH_TRUE = 103,
    POP = 110,
    LOAD_LOCAL0 = 200,
    LOAD_LOCAL1 = 201,
    LOAD_LOCAL2 = 202,
    LOAD_LOCAL3 = 203,
    LOAD_LOCAL4 = 204,
    LOAD_LOCAL5 = 205,
    LOAD_LOCAL6 = 206,
    LOAD_LOCAL7 = 207,
    LOAD_LOCAL8 = 208,
    LOAD_LOCAL9 = 209,
    LOAD_LOCAL10 = 210,
    LOAD_LOCAL11 = 211,
    LOAD_LOCAL12 = 212,
    LOAD_LOCAL13 = 213,
    LOAD_LOCAL14 = 214,
    LOAD_LOCAL15 = 215,
    LOAD_LOCAL = 220,
    STORE_LOCAL = 221,
    STORE_LOCAL_NON_TOP = 222,
    LOAD_UPVALUE = 230,
    STORE_UPVALUE = 231,
    STORE_UPVALUE_NON_TOP = 232,
    LOAD_FIELD = 240,
    STORE_FIELD = 241,
    LOAD_THIS_FIELD = 250,
    STORE_THIS_FIELD = 251,
    LOAD_PKG_VAR = 260,
    STORE_PKG_VAR = 261,
    STORE_PKG_VAR_NON_TOP = 262,
    STORE_INDEXING = 270,
    STORE_INDEXING_NON_TOP = 271,
    PRE_CALL = 300,
    CALL = 301,
    CALL_PRIM_1_1 = 310,
    CALL_PRIM_2_1 = 311,
    JUMP = 400,
    LOOP = 401,
    JUMP_IF = 402,
    AND = 403,
    OR = 404,
    CLOSE_UPVALUE = 405,
    RETURN = 406,
    NEW_CLOSURE = 501,
    NEW_STRUCT = 502,
    NEW_SLICE = 503,
    NEW_MAP = 504,
}

impl Opcode {
    pub fn get_load_local(i: OpIndex) -> Opcode {
        match i {
            0 => Opcode::LOAD_LOCAL0,
            1 => Opcode::LOAD_LOCAL1,
            2 => Opcode::LOAD_LOCAL2,
            3 => Opcode::LOAD_LOCAL3,
            4 => Opcode::LOAD_LOCAL4,
            5 => Opcode::LOAD_LOCAL5,
            6 => Opcode::LOAD_LOCAL6,
            7 => Opcode::LOAD_LOCAL7,
            8 => Opcode::LOAD_LOCAL8,
            9 => Opcode::LOAD_LOCAL9,
            10 => Opcode::LOAD_LOCAL10,
            11 => Opcode::LOAD_LOCAL11,
            12 => Opcode::LOAD_LOCAL12,
            13 => Opcode::LOAD_LOCAL13,
            14 => Opcode::LOAD_LOCAL14,
            15 => Opcode::LOAD_LOCAL15,
            _ => Opcode::LOAD_LOCAL,
        }
    }

    pub fn load_local_index(&self) -> OpIndex {
        (*self as i16 - Opcode::LOAD_LOCAL0 as i16) as OpIndex
    }

    pub fn property(&self) -> (&str, i8) {
        match self {
            Opcode::PUSH_CONST => ("PUSH_CONST", 1),
            Opcode::PUSH_NIL => ("PUSH_NIL", 1),
            Opcode::PUSH_FALSE => ("PUSH_FALSE", 1),
            Opcode::PUSH_TRUE => ("PUSH_TRUE", 1),
            Opcode::POP => ("POP", -1),
            Opcode::LOAD_LOCAL0 => ("LOAD_LOCAL0", 1),
            Opcode::LOAD_LOCAL1 => ("LOAD_LOCAL1", 1),
            Opcode::LOAD_LOCAL2 => ("LOAD_LOCAL2", 1),
            Opcode::LOAD_LOCAL3 => ("LOAD_LOCAL3", 1),
            Opcode::LOAD_LOCAL4 => ("LOAD_LOCAL4", 1),
            Opcode::LOAD_LOCAL5 => ("LOAD_LOCAL5", 1),
            Opcode::LOAD_LOCAL6 => ("LOAD_LOCAL6", 1),
            Opcode::LOAD_LOCAL7 => ("LOAD_LOCAL7", 1),
            Opcode::LOAD_LOCAL8 => ("LOAD_LOCAL8", 1),
            Opcode::LOAD_LOCAL9 => ("LOAD_LOCAL9", 1),
            Opcode::LOAD_LOCAL10 => ("LOAD_LOCAL10", 1),
            Opcode::LOAD_LOCAL11 => ("LOAD_LOCAL11", 1),
            Opcode::LOAD_LOCAL12 => ("LOAD_LOCAL12", 1),
            Opcode::LOAD_LOCAL13 => ("LOAD_LOCAL13", 1),
            Opcode::LOAD_LOCAL14 => ("LOAD_LOCAL14", 1),
            Opcode::LOAD_LOCAL15 => ("LOAD_LOCAL15", 1),
            Opcode::LOAD_LOCAL => ("LOAD_LOCAL", 1),
            Opcode::STORE_LOCAL => ("STORE_LOCAL", 0),
            Opcode::STORE_LOCAL_NON_TOP => ("STORE_LOCAL_NON_TOP", 0),
            Opcode::LOAD_UPVALUE => ("LOAD_LOCAL", 1),
            Opcode::STORE_UPVALUE => ("STORE_UPVALUE", 0),
            Opcode::STORE_UPVALUE_NON_TOP => ("STORE_UPVALUE_NON_TOP", 0),
            Opcode::LOAD_FIELD => ("LOAD_FIELD", 1),
            Opcode::STORE_FIELD => ("STORE_FIELD", 0),
            Opcode::LOAD_THIS_FIELD => ("LOAD_THIS_FIELD", 1),
            Opcode::STORE_THIS_FIELD => ("STORE_THIS_FIELD", 0),
            Opcode::LOAD_PKG_VAR => ("LOAD_PKG_VAR", 1),
            Opcode::STORE_PKG_VAR => ("STORE_PKG_VAR", 0),
            Opcode::STORE_PKG_VAR_NON_TOP => ("STORE_PKG_VAR_NON_TOP", 0),
            Opcode::STORE_INDEXING => ("STORE_INDEXING", 0),
            Opcode::STORE_INDEXING_NON_TOP => ("STORE_INDEXING_NON_TOP", 0),
            Opcode::PRE_CALL => ("PRE_CALL", -128),
            Opcode::CALL => ("CALL", -128),
            Opcode::CALL_PRIM_1_1 => ("CALL_PRIM_1_1", 0),
            Opcode::CALL_PRIM_2_1 => ("CALL_PRIM_2_1", -1),
            Opcode::JUMP => ("JUMP", 0),
            Opcode::LOOP => ("LOOP", 0),
            Opcode::JUMP_IF => ("JUMP_IF", -1),
            Opcode::AND => ("AND", -1),
            Opcode::OR => ("OR", -1),
            Opcode::CLOSE_UPVALUE => ("CLOSE_UPVALUE", -1),
            Opcode::RETURN => ("RETURN", 0),
            Opcode::NEW_CLOSURE => ("NEW_CLOSURE", 1),
            Opcode::NEW_STRUCT => ("NEW_STRUCT", 0),
            Opcode::NEW_SLICE => ("NEW_SLICE", 0),
            Opcode::NEW_MAP => ("NEW_MAP", 0),
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

#[derive(Clone, Copy, Debug)]
pub enum CodeData {
    Code(Opcode),
    Data(OpIndex),
}

impl CodeData {
    pub fn unwrap_code(&self) -> &Opcode {
        match self {
            CodeData::Code(code) => code,
            CodeData::Data(_) => unreachable!(),
        }
    }

    pub fn unwrap_data(&self) -> &OpIndex {
        match self {
            CodeData::Code(_) => unreachable!(),
            CodeData::Data(data) => data,
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
}
