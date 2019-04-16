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

const MAX_ARG_Bx:u32 =  (1 << SIZE_Bx) - 1;
const MAX_ARG_sBx:i32 = MAX_ARG_Bx as i32 >> 1;        
const MAX_ARG_Ax:u32 =  (1 << SIZE_Ax) - 1;

const MAX_ARG_A:u32 =   (1 << SIZE_A) - 1;
const MAX_ARG_B:u32 =   (1 << SIZE_B) - 1;
const MAX_ARG_C:u32 =   (1 << SIZE_C) - 1;

// this bit 1 means constant (0 means register)
const BIT_R_K:u32 =		1 << (SIZE_B - 1);
// for debugging only
const MAX_INDEX_R_K:u32 = BIT_R_K - 1;

// invalid register that fits in 8 bits
const NO_REG:u32 = MAX_ARG_A;


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

#[derive(Clone, Debug)]
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
    pub fn get_sbx(&self) -> i32 {self.get_bx() as i32 - MAX_ARG_sBx}

    #[inline]
    pub fn set_sbx(&mut self, v: i32) {
        self.set_bx((v + MAX_ARG_sBx) as u32)
    }

    // test whether value is a constant
    #[inline]
    pub fn is_k(x: u32) -> bool {
        (x & BIT_R_K) > 0
    }

    // gets the index of the constant
    #[inline]
    pub fn index_k(r: u32) -> u32 {
        r & (!BIT_R_K)
    }

    // code a constant index as a RK value
    #[inline]
    pub fn rk_as_k(x: u32) -> u32 {
        x | BIT_R_K
    }

    pub fn get_props(&self) -> &'static OpProp {
        &OP_PROPS[self.get_opcode() as usize]
    }
}

impl fmt::Display for Instruction {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let prop = self.get_props();
        let a = self.get_a();
        let b = self.get_b();
        let c = self.get_c();
        let bx = self.get_bx();
        let sbx = self.get_sbx();
        let ax = self.get_ax();
        match prop.op_mode {
            OpMode::ABC => {
                write!(f, "{} A:{},B:{},C:{}; ", prop.name, a, b, c)?
            }
            OpMode::ABx => {
                write!(f, "{} A:{},Bx:{}; ", prop.name, a, bx)?
            }
            OpMode::AsBx => {
                write!(f, "{} A:{},sBx:{}; ", prop.name, a, sbx)?
            }
            OpMode::Ax => {
                write!(f, "{} Ax:{}; ", prop.name, ax)?
            }
        }
        match self.get_opcode() {
            OP_MOVE => {write!(f, "R({}) := R({})", a, b)}
            OP_LOADK => {write!(f, "R({}) := Kst({})", a, bx)}
            OP_LOADKX => {write!(f, "R({}) := Kst(extra arg)", a)}                          
            OP_LOADBOOL => {write!(f, "R({}) := (Bool){}; if ({}) pc++", a , b, c)}                   
            OP_LOADNIL => {write!(f, "R({0}), R({0}+1), ..., R({0}+{1}) := nil", a, b)}                
            OP_GETUPVAL => {write!(f, "R({}) := UpValue[{}]", a, b)}                              
            OP_GETGLOBAL => {write!(f, "R({}) := UpValue[{}][RK({})]", a, b, c)}                     
            OP_GETTABLE => {write!(f, "R({}) := R({})[RK({})]", a, b, c)}                    
            OP_SETGLOBAL => {write!(f, "UpValue[{}][RK({})] := RK({})", a, b, c)}                     
            OP_SETUPVAL => {write!(f, "UpValue[{1}] := R({0})", a, b)}               
            OP_SETTABLE => {write!(f, "R({})[RK({})] := RK({})", a, b, c)}                            
            OP_NEWTABLE => {write!(f, "R({}) := NewObj (size = {},{})", a, b, c)}                        
            OP_SELF => {write!(f, "R({0}+1) := R({1}); R({0}) := R({1})[RK({2})]", a, b, c)}           
            OP_ADD => {write!(f, "R({}) := RK({}) + RK({})", a, b, c)}                           
            OP_SUB => {write!(f, "R({}) := RK({}) - RK({})", a, b, c)}                           
            OP_MUL => {write!(f, "R({}) := RK({}) * RK({})", a, b, c)}                           
            OP_MOD => {write!(f, "R({}) := RK({}) % RK({})", a, b, c)}                          
            OP_POW => {write!(f, "R({}) := RK({}) ^ RK({})", a, b, c)}                           
            OP_DIV => {write!(f, "R({}) := RK({}) / RK({})", a, b, c)}                           
            OP_IDIV => {write!(f, "R({}) := RK({}) // RK({})", a, b, c)}                          
            OP_BAND => {write!(f, "R({}) := RK({}) & RK({})", a, b, c)}                           
            OP_BOR => {write!(f, "R({}) := RK({}) | RK({})", a, b, c)}                           
            OP_BXOR => {write!(f, "R({}) := RK({}) ~ RK({})", a, b, c)}                           
            OP_SHL => {write!(f, "R({}) := RK({}) << RK({})", a, b, c)}                          
            OP_SHR => {write!(f, "R({}) := RK({}) >> RK({})", a, b, c)}                          
            OP_UNM => {write!(f, "R({}) := -R({})", a, b)}                                   
            OP_BNOT => {write!(f, "R({}) := ~R({})", a, b)}                                   
            OP_NOT => {write!(f, "R({}) := not R({})", a, b)}                                
            OP_LEN => {write!(f, "R({}) := length of R({})", a, b)}                          
            OP_CONCAT => {write!(f, "R({}) := R({}).. ... ..R({})", a, b, c)}                       
            OP_JMP => {write!(f, "pc+={1}; if ({0}) close all upvalues >= R({0} - 1)", a, sbx)}  
            OP_EQ => {write!(f, "if ((RK({1}) == RK({2})) ~= {0}) then pc++", a, b, c)}            
            OP_LT => {write!(f, "if ((RK({1}) <  RK({2})) ~= {0}) then pc++", a, b, c)}            
            OP_LE => {write!(f, "if ((RK({1}) <= RK({2})) ~= {0}) then pc++", a, b, c)}            
            OP_TEST => {write!(f, "if not (R({}) <=> {}) then pc++", a, c)}                   
            OP_TESTSET => {write!(f, 
                "if (R({1}) <=> {2}) then R({0}) := R({1}) else pc++", a, b, c)}     
            OP_CALL => {write!(f, 
                "R({0}), ... ,R({0}+{2}-2) := R({0})(R({0}+1), ... ,R({0}+{1}-1))", a, b, c)} 
            OP_TAILCALL => {write!(f, "return R({0})(R({0}+1), ... ,R({0}+{1}-1))", a, b)}              
            OP_RETURN => {write!(f, "return R({0}), ... ,R({0}+{1}-2)", a, b)}      
            OP_FORLOOP => {
                write!(f, "R({0})+=R({0}+2); ", a)?;
                write!(f, "if R({0}) <?= R({0}+1) then {{ pc+={1}; R({0}+3)=R({0}) }}", a, sbx)
            }
            OP_FORPREP => {write!(f, "R({0})-=R({0}+2); pc+={1}", a, sbx)}
            OP_TFORLOOP => {write!(f, 
                "if R({0}+1) ~= nil then {{ R({0})=R({0}+1); pc += {1} }}", a, sbx)}
            OP_SETLIST => {write!(f, 
                "R({0})[({2}-1)*FPF+i] := R({0}+i), 1 <= i <= {1}", a, b, c)}   
            OP_CLOSE => {write!(f, "close all variables in the stack up to (>=) R({})", a)}                          
            OP_CLOSURE => {write!(f, "R({}) := closure(KPROTO[{}])", a, bx)}                     
            OP_VARARG => {write!(f, "R({0}), R({0}+1), ..., R({0}+{1}-2) = vararg", a, b)}            
            OP_EXTRAARG => {write!(f, "extra (larger) argument for previous opcode" )}
            _ => unreachable!()
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
        i.set_sbx(-222);
        assert!(i.get_sbx() == -222);
        i.set_bx(2);
        assert!(i.get_opcode() == OP_LOADK);
        println!("instruction {}\n", i); 
    }
}