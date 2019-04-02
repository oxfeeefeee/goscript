#![allow(dead_code)]
#![allow(non_upper_case_globals)]
use std::fmt;
use super::opcode::*;

// Based on lua instructions:
/*===========================================================================
  We assume that instructions are unsigned numbers.
  All instructions have an opcode in the first 6 bits.
  Instructions can have the following fields:
        'A' : 8 bits
        'B' : 9 bits
        'C' : 9 bits
        'Ax' : 26 bits ('A', 'B', and 'C' together)
        'Bx' : 18 bits ('B' and 'C' together)
        'sBx' : signed Bx

  A signed argument is represented in excess K; that is, the number
  value is the unsigned value minus K. K is exactly the maximum value
  for that argument (so that -max is represented by 0, and +max is
  represented by 2*max), which is half the maximum for the corresponding
  unsigned argument.
===========================================================================*/

const SIZE_C:u32 =		9;
const SIZE_B:u32 =		9;
const SIZE_Bx:u32 =		SIZE_C + SIZE_B;
const SIZE_A:u32 =		8;
const SIZE_Ax:u32 =		SIZE_C + SIZE_B + SIZE_A;

const SIZE_OP:u32 =		6;

const POS_OP:u32 =		0;
const POS_A:u32 =		POS_OP + SIZE_OP;
const POS_C:u32 =		POS_A + SIZE_A;
const POS_B:u32 =		POS_C + SIZE_C;
const POS_Bx:u32 =		POS_C;
const POS_Ax:u32 =		POS_A;

const MAXARG_Bx:u32 =       (1 << SIZE_Bx) - 1;
const MAXARG_sBx:i32 =      MAXARG_Bx as i32 >> 1;        
const MAXARG_Ax:u32 =   	(1 << SIZE_Ax) - 1;

const MAXARG_A:u32 =        (1 << SIZE_A) - 1;
const MAXARG_B:u32 =        (1 << SIZE_B) - 1;
const MAXARG_C:u32 =        (1 << SIZE_C) - 1;

// creates a mask with 'n' 1 bits at position 'p'
macro_rules! mask1 { ($n:expr, $p:expr) => { (!((!(0 as u32))<< $n)) << $p }; }

// creates a mask with 'n' 0 bits at position 'p'
macro_rules! mask0 { ($n:expr, $p:expr) => { !mask1!($n, $p); }; }

macro_rules! get_arg {
    ($i:expr, $pos:expr, $size:expr) => { ($i >> $pos) & mask1!($size, 0) }; 
}

macro_rules! set_arg {
    ($i:expr, $v:expr, $pos:expr, $size:expr) => {
        $i = ($i & mask0!($size, $pos)) | (($v << $pos) & mask1!($size, $pos))
    };
}

pub struct Instruction(u32);

impl Instruction {
    pub fn new_abc(op: u32, a: u32, b: u32, c: u32) -> Instruction {
        Instruction((op << POS_OP) | (a << POS_A) | (b << POS_B) | (c << POS_C))
    }

    pub fn new_abx(op: u32, a: u32, bx: u32) -> Instruction {
        Instruction((op << POS_OP) | (a << POS_A) | (bx << POS_Bx))
    }

    pub fn new_ax(op: u32, ax: u32) -> Instruction {
        Instruction((op << POS_OP) | (ax << POS_Ax))
    }

    #[inline]  
    pub fn get_opcode(&self) -> Opcode {
        ((self.0) >> POS_OP) & mask1!(SIZE_OP, 0)
    } 

    #[inline]
    pub fn set_opcode(&mut self, op: Opcode) {
        self.0 = (self.0 & mask0!(SIZE_OP, POS_OP)) |
            ((op << POS_OP) & mask1!(SIZE_OP, POS_OP))
    }

    #[inline]
    pub fn get_a(&self) -> u32 {get_arg!(self.0, POS_A, SIZE_A)}

    #[inline]
    pub fn set_a(&mut self, v: u32) {set_arg!(self.0, v, POS_A, SIZE_A)}

    #[inline]
    pub fn get_b(&self) -> u32 {get_arg!(self.0, POS_B, SIZE_B)}

    #[inline]
    pub fn set_b(&mut self, v: u32) {set_arg!(self.0, v, POS_B, SIZE_B)}

    #[inline]
    pub fn get_c(&self) -> u32 {get_arg!(self.0, POS_C, SIZE_C)}

    #[inline]
    pub fn set_c(&mut self, v: u32) {set_arg!(self.0, v, POS_C, SIZE_C)}

    #[inline]
    pub fn get_ax(&self) -> u32 {get_arg!(self.0, POS_Ax, SIZE_Ax)}

    #[inline]
    pub fn set_ax(&mut self, v: u32) {set_arg!(self.0, v, POS_Ax, SIZE_Ax)}

    #[inline]
    pub fn get_bx(&self) -> u32 {get_arg!(self.0, POS_Bx, SIZE_Bx)}

    #[inline]
    pub fn set_bx(&mut self, v: u32) {set_arg!(self.0, v, POS_Bx, SIZE_Bx)}

    #[inline]
    pub fn get_sbx(&self) -> i32 {self.get_bx() as i32 - MAXARG_sBx}

    #[inline]
    pub fn set_sbx(&mut self, v: i32) {
        self.set_bx((v + MAXARG_sBx) as u32)
    }
}

impl fmt::Display for Instruction {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let prop = &OP_PROPS[self.get_opcode() as usize];
        let a = self.get_a();
        let b = self.get_b();
        let c = self.get_c();
        let bx = self.get_bx();
        let sbx = self.get_sbx();
        let ax = self.get_ax();
        match prop.op_mode {
            OpMode::ABC => {
                write!(f, "{} A:{},B:{},C:{}", prop.name, a, b, c)
            }
            OpMode::ABx => {
                write!(f, "{} A:{},Bx:{}", prop.name, a, bx)
            }
            OpMode::AsBx => {
                write!(f, "{} A:{},sBx:{}", prop.name, a, sbx)
            }
            OpMode::Ax => {
                write!(f, "{} Ax:{}", prop.name, ax)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
	fn test_instruction() {
        let mut i = Instruction(0);
        i.set_opcode(OP_LOADK);
        i.set_a(1);
        i.set_bx(2);
        //assert!(i.get_opcode() == OP_LOADK);
        println!("instruction {}\n", i); 
    }
}