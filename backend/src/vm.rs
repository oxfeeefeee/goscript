#![allow(dead_code)]
use std::rc::{Rc};
use std::collections::HashMap;
use super::types::*;
use super::proto::*;
use super::opcode::*;

type PanicFunc = fn(s: &State);

pub struct GlobalState {
    globals:    HashMap<GosValue, GosValue>,
    panic:      PanicFunc,
}

/*
https://the-ravi-programming-language.readthedocs.io/en/latest/lua_bytecode_reference.html
Caller   One fixed arg               Two variable args and 1     Two variable args and no
                                     fixed arg                   fixed args
R(A)     CI->func  [ function    ]   CI->func  [ function    ]   CI->func [ function   ]
R(A+1)   CI->base  [ fixed arg 1 ]             [ var arg 1   ]            [ var arg 1  ]
R(A+2)             [ local 1     ]             [ var arg 2   ]            [ var arg 2  ]
R(A+3)                               CI->base  [ fixed arg 1 ]   CI->base [ local 1    ]
R(A+4)                                         [ local 1     ]
*/
#[derive(Clone)]
pub struct CallInfo<'c> {
    index: usize,   // self-index in CallFrames
    closure: Option<&'c Closure>,    
    pc: usize,
    base: u32,        // CI->func in the table above
    local_base: u32,  // CI->base in the table above
    return_base: usize,
    arg_count: usize,
    ret_count: usize,
    tail_call: usize,
}

pub struct CallFrames<'c> {
    frames: Vec<CallInfo<'c>>,
    pointer: usize,
}

pub struct RegStack {
    stack:  Vec<GosValue>,
    top: usize,
}

pub struct State<'p, 'c> {
    g_state: &'static GlobalState,
    parent: Option<&'p State<'p, 'c>>,
    env: HashMap<GosValue, GosValue>,
    up_values: Vec<UpValue>,
    call_frames: CallFrames<'c>,
    reg_stack: RegStack,
    current_frame: usize,
}

impl<'c> CallFrames<'c> {
    pub fn new(size: usize) -> CallFrames<'c> {
        let frames = vec![CallInfo{
            index: 0,
            closure: None,
            pc: 0,
            base: 0,
            local_base: 0,
            return_base: 0,
            arg_count: 0,
            ret_count: 0,
            tail_call: 0,
        }; size];
        CallFrames{frames: frames, pointer: 0}
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pointer == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        self.pointer = 0;
    }

    #[inline]
    pub fn push(&mut self, cf: CallInfo<'c>) {
        self.frames[self.pointer] = cf;
        self.frames[self.pointer].index = self.pointer;
        self.pointer += 1;
    }

    #[inline]
    pub fn pop(&mut self) -> &mut CallInfo<'c> {
        self.pointer -= 1;
        &mut self.frames[self.pointer]
    }

    #[inline]
    pub fn get(&mut self, index: usize) -> &mut CallInfo<'c> {
        &mut self.frames[index]
    }

    pub fn remove_at(&mut self, index: usize) {
        for i in index..self.pointer-1 {
            self.frames[i] = self.frames[i+1].clone();
            self.frames[i].index = i;
            self.pointer = i;
        }
        self.pointer += 1;
    }

    #[inline]
    pub fn try_last<'b>(&'b mut self) -> Option<&'b mut CallInfo<'c>> {
        if self.pointer > 0 {
            Some(&mut self.frames[self.pointer-1])
        } else {
            None
        }
    }

    #[inline]
    pub fn pointer(&self) -> usize {
        self.pointer
    }

    #[inline]
    pub fn set_pointer(&mut self, p: usize) {
        self.pointer = p;
    }
}

impl RegStack {
    pub fn new(size: usize) -> RegStack {
        RegStack{
            stack: vec![GosValue::Nil;size],
            top: 0,
        }
    }

    #[inline]
    pub fn top(&self) -> usize {
        self.top
    }

    #[inline]
    pub fn set_top(&mut self, top: usize) {
        let (mut from, mut to) = (self.top, top);
        if from > to {
            from = top;
            to = self.top;
        }
        for i in from..to {
            self.stack[i] = GosValue::Nil;
        }
    }

    #[inline]
    pub fn get_top(&self) -> &GosValue {
        &self.stack[self.top] 
    }

    #[inline]
    pub fn get(&self, reg: u32) -> &GosValue {
        &self.stack[reg as usize] 
    }

    #[inline]
    pub fn set(&mut self, reg: u32, v: GosValue) {
        self.stack[reg as usize] = v;
        self.top = reg as usize + 1;
    }

    #[inline]
    pub fn push(&mut self, v: GosValue) {
        self.stack[self.top] = v;
        self.top += 1;
    }

    #[inline]
    pub fn pop(&mut self) -> GosValue {
        let v = self.stack[self.top].clone();
        self.stack[self.top] = GosValue::Nil;
        v
    }
    
    #[inline]
    pub fn fill_nil(&mut self, start: usize, count: usize) {
        for i in start..start+count {
            self.stack[i] = GosValue::Nil;
        }
        self.top = start + count;
    }
}

impl<'p, 'c> State<'p, 'c> {
    fn exec(&mut self) {
        match self.call_frames.try_last() {
            Some(ci) => { self.current_frame = ci.index; },
            None => { return; } 
        }
        let ci = self.call_frames.get(self.current_frame);
        let func = ci.closure.unwrap();
        loop {
            let reg = &mut self.reg_stack;
            let inst = &func.proto.code[ci.pc];
            ci.pc += 1;
            let base = ci.local_base;
            match inst.get_opcode() {
                OP_MOVE => {
                    let a = inst.get_a();
                    let b = inst.get_b();
                    reg.set(base + a, reg.get(base + b).clone())
                }
                OP_LOADK => {
                    let a = inst.get_a();
                    let bx = inst.get_bx() as usize;
                    reg.set(base + a, func.proto.constants[bx].clone());
                }
                OP_LOADBOOL => {
                    let a = inst.get_a();
                    let b = inst.get_b();
                    let c = inst.get_c();
                    let val = if b == 0 {true} else {false};
                    reg.set(base + a, GosValue::Bool(val));
                    if c != 0 {
                        ci.pc += 1;
                    }
                }
                OP_LOADNIL => {
                    let a = inst.get_a();
                    let b = inst.get_b();
                    for i in base + a .. base + b + 1 {
                        reg.set(i, GosValue::Nil);
                    }
                }
                _ => {panic!("invalid opcode!")}
            }
        }
    }
}