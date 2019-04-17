#![allow(dead_code)]
#![allow(non_camel_case_types)]

pub type Opcode = u32;

pub const OP_MOVE: Opcode =  0;//      A B     R(A) := R(B)                                    
pub const OP_LOADK: Opcode =  1;//     A Bx    R(A) := Kst(Bx)                                 
pub const OP_LOADKX: Opcode =  2;//    A       R(A) := Kst(extra arg)                          
pub const OP_LOADBOOL: Opcode =  3;//  A B C   R(A) := (Bool)B; if (C) pc++                    
pub const OP_LOADNIL: Opcode =  4;//   A B     R(A), R(A+1), ..., R(A+B) := nil 

pub const OP_GETUPVAL: Opcode =  5;//  A B     R(A) := UpValue[B]     
pub const OP_GETGLOBAL: Opcode =  6;// A Bx	   R(A) := Gbl[Kst(Bx)]	
pub const OP_GETTABLE: Opcode =  7;//  A B C   R(A) := R(B)[RK(C)]
pub const OP_SETGLOBAL: Opcode =  8;// A Bx    Gbl[Kst(Bx)] := R(A)				
pub const OP_SETUPVAL: Opcode =  9;//  A B     UpValue[B] := R(A)                              
pub const OP_SETTABLE: Opcode = 10;//  A B C   R(A)[RK(B)] := RK(C)

pub const OP_NEWTABLE: Opcode = 11;//  A B C   R(A) := {} (size = B,C)                         

pub const OP_SELF: Opcode = 12;//      A B C   R(A+1) := R(B); R(A) := R(B)[RK(C)]             

pub const OP_ADD: Opcode = 13;//       A B C   R(A) := RK(B) + RK(C)                           
pub const OP_SUB: Opcode = 14;//       A B C   R(A) := RK(B) - RK(C)                           
pub const OP_MUL: Opcode = 15;//       A B C   R(A) := RK(B) * RK(C)                           
pub const OP_MOD: Opcode = 16;//       A B C   R(A) := RK(B) % RK(C)                           
pub const OP_POW: Opcode = 17;//       A B C   R(A) := RK(B) ^ RK(C)                           
pub const OP_DIV: Opcode = 18;//       A B C   R(A) := RK(B) / RK(C)                           
pub const OP_IDIV: Opcode = 19;//      A B C   R(A) := RK(B) // RK(C)                          
pub const OP_BAND: Opcode = 20;//      A B C   R(A) := RK(B) & RK(C)                           
pub const OP_BOR: Opcode = 21;//       A B C   R(A) := RK(B) | RK(C)                           
pub const OP_BXOR: Opcode = 22;//      A B C   R(A) := RK(B) ~ RK(C)                           
pub const OP_SHL: Opcode = 23;//       A B C   R(A) := RK(B) << RK(C)                          
pub const OP_SHR: Opcode = 24;//       A B C   R(A) := RK(B) >> RK(C)                          
pub const OP_UNM: Opcode = 25;//       A B     R(A) := -R(B)                                   
pub const OP_BNOT: Opcode = 26;//      A B     R(A) := ~R(B)                                   
pub const OP_NOT: Opcode = 27;//       A B     R(A) := not R(B)                                
pub const OP_LEN: Opcode = 28;//       A B     R(A) := length of R(B)                          

pub const OP_CONCAT: Opcode = 29;//    A B C   R(A) := R(B).. ... ..R(C)                       

pub const OP_JMP: Opcode = 30;//       A sBx   pc+=sBx
pub const OP_EQ: Opcode = 31;//        A B C   if ((RK(B) == RK(C)) ~= A) then pc++            
pub const OP_LT: Opcode = 32;//        A B C   if ((RK(B) <  RK(C)) ~= A) then pc++            
pub const OP_LE: Opcode = 33;//        A B C   if ((RK(B) <= RK(C)) ~= A) then pc++            

pub const OP_TEST: Opcode = 34;//      A C     if not (R(A) <=> C) then pc++                   
pub const OP_TESTSET: Opcode = 35;//   A B C   if (R(B) <=> C) then R(A) := R(B) else pc++     

pub const OP_CALL: Opcode = 36;//      A B C   R(A), ... ,R(A+C-2) := R(A)(R(A+1), ... ,R(A+B-1)) 
pub const OP_TAILCALL: Opcode = 37;//  A B C   return R(A)(R(A+1), ... ,R(A+B-1))              
pub const OP_RETURN: Opcode = 38;//    A B     return R(A), ... ,R(A+B-2)      (see note)      

pub const OP_FORLOOP: Opcode = 39;//   A sBx   R(A)+=R(A+2);
                          //           if R(A) <?= R(A+1) then { pc+=sBx; R(A+3)=R(A) }
pub const OP_FORPREP: Opcode = 40;//   A sBx   R(A)-=R(A+2); pc+=sBx                           

pub const OP_TFORLOOP: Opcode = 41;//  A sBx   if R(A+1) ~= nil then { R(A)=R(A+1); pc += sBx }

pub const OP_SETLIST: Opcode = 42;//   A B C   R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B        

pub const OP_CLOSE: Opcode = 43; //	   A 	   close all variables in the stack up to (>=) R(A)
pub const OP_CLOSURE: Opcode = 44;//   A Bx    R(A) := closure(KPROTO[Bx])                     

pub const OP_VARARG: Opcode = 45;//    A B     R(A), R(A+1), ..., R(A+B-2) = vararg            

pub const OP_EXTRAARG: Opcode = 46;//   Ax     extra (larger) argument for previous opcode     


pub enum OpMode {
    ABC = 0,
    ABx = 1,
    AsBx = 2,
    Ax = 3, 
}

pub enum OpArgMode {
    N = 0, // argument is not used
    U = 1, // argument is used
    R = 2, // argument is a register or a jump offset
    K = 3, // argument is a constant or register/constant
}

pub struct OpProp {
    pub name: &'static str,
    pub test: bool,
    pub set_a: bool,
    pub b_mode: OpArgMode,
    pub c_mode: OpArgMode,
    pub op_mode: OpMode,
}

macro_rules! prop {
    ($name:expr, $test:expr, $seta:expr, $bmode:expr, $cmode:expr, $opmode:expr) => {
        OpProp{name: $name, test: $test, set_a: $seta, 
            b_mode: $bmode, c_mode: $cmode, op_mode: $opmode}
    };
}

pub const OP_PROPS: &[OpProp] = &[
    prop!("MOVE", false, true, OpArgMode::R, OpArgMode::N, OpMode::ABC),		
    prop!("LOADK", false, true, OpArgMode::K, OpArgMode::N, OpMode::ABx),		
    prop!("LOADKX", false, true, OpArgMode::N, OpArgMode::N, OpMode::ABx),		
    prop!("LOADBOOL", false, true, OpArgMode::U, OpArgMode::U, OpMode::ABC),	
    prop!("LOADNIL", false, true, OpArgMode::U, OpArgMode::N, OpMode::ABC),		
    prop!("GETUPVAL", false, true, OpArgMode::U, OpArgMode::N, OpMode::ABC),	
    prop!("GETTABUP", false, true, OpArgMode::U, OpArgMode::K, OpMode::ABC),	
    prop!("GETTABLE", false, true, OpArgMode::R, OpArgMode::K, OpMode::ABC),	
    prop!("SETTABUP", false, false, OpArgMode::K, OpArgMode::K, OpMode::ABC),	
    prop!("SETUPVAL", false, false, OpArgMode::U, OpArgMode::N, OpMode::ABC),	
    prop!("SETTABLE", false, false, OpArgMode::K, OpArgMode::K, OpMode::ABC),	
    prop!("NEWTABLE", false, true, OpArgMode::U, OpArgMode::U, OpMode::ABC),	
    prop!("SELF", false, true, OpArgMode::R, OpArgMode::K, OpMode::ABC),		
    prop!("ADD", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("SUB", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("MUL", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("MOD", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("POW", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("DIV", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("IDIV", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),	
    prop!("BAND", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),	
    prop!("BOR", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("BXOR", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),	
    prop!("SHL", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("SHR", false, true, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("UNM", false, true, OpArgMode::R, OpArgMode::N, OpMode::ABC),		
    prop!("BNOT", false, true, OpArgMode::R, OpArgMode::N, OpMode::ABC),	
    prop!("NOT", false, true, OpArgMode::R, OpArgMode::N, OpMode::ABC),		
    prop!("LEN", false, true, OpArgMode::R, OpArgMode::N, OpMode::ABC),		
    prop!("CONCAT", false, true, OpArgMode::R, OpArgMode::R, OpMode::ABC),	
    prop!("JMP", false, false, OpArgMode::R, OpArgMode::N, OpMode::AsBx),	
    prop!("EQ", true, false, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("LT", true, false, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("LE", true, false, OpArgMode::K, OpArgMode::K, OpMode::ABC),		
    prop!("TEST", true, false, OpArgMode::N, OpArgMode::U, OpMode::ABC),	
    prop!("TESTSET", true, true, OpArgMode::R, OpArgMode::U, OpMode::ABC),	
    prop!("CALL", false, true, OpArgMode::U, OpArgMode::U, OpMode::ABC),	
    prop!("TAILCALL", false, true, OpArgMode::U, OpArgMode::U, OpMode::ABC),	
    prop!("RETURN", false, false, OpArgMode::U, OpArgMode::N, OpMode::ABC),		
    prop!("FORLOOP", false, true, OpArgMode::R, OpArgMode::N, OpMode::AsBx),	
    prop!("FORPREP", false, true, OpArgMode::R, OpArgMode::N, OpMode::AsBx),	
    prop!("TFORCALL", false, false, OpArgMode::N, OpArgMode::U, OpMode::ABC),	
    prop!("TFORLOOP", false, true, OpArgMode::R, OpArgMode::N, OpMode::AsBx),	
    prop!("SETLIST", false, false, OpArgMode::U, OpArgMode::U, OpMode::ABC),	
    prop!("CLOSURE", false, true, OpArgMode::U, OpArgMode::N, OpMode::ABx),		
    prop!("VARARG", false, true, OpArgMode::U, OpArgMode::N, OpMode::ABC),		
    prop!("EXTRAARG", false, false, OpArgMode::U, OpArgMode::U, OpMode::Ax),
    ];


#[cfg(test)]
mod test {
    use super::*;

    #[test]
	fn test_opcode() {
        println!("opcode\n"); 
        assert!(OP_PROPS[OP_EXTRAARG as usize].name == "EXTRAARG");
        assert!(OP_PROPS[OP_UNM as usize].name == "UNM");
        assert!(OP_PROPS[OP_EXTRAARG as usize].name == "EXTRAARG");

        let i:u32 = 16;
        match i {
            OP_MOD => {print!("MOD\n");}
            _ => {print!("notfound\n");}
        }
    }
}