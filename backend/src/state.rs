#![allow(dead_code)]
use std::collections::HashMap;
use super::types::GosValue;

type PanicFunc = fn(s: &State);

pub struct GlobalState {
    globals:    HashMap<GosValue, GosValue>,
    panic:      PanicFunc,
}

pub enum UpValue {
    Open(u32),
    Closed(GosValue),
}

#[derive(Clone)]
pub struct CallInfo {
    index: usize,   // self-index in CallFrames
    func: usize,    // function index in RegStack
    pc: usize,
    base: usize,
    local_base: usize,
    return_base: usize,
    arg_count: usize,
    ret_count: usize,
    tail_call: usize,
}

pub struct CallFrames {
    frames: Vec<CallInfo>,
    pointer: usize,
}

pub struct RegStack {
    stack:  Vec<GosValue>,
    top: usize,
}

pub struct State<'a> {
    g_state: &'static GlobalState,
    parent: Option<&'a State<'a>>,
    env: HashMap<GosValue, GosValue>,
    up_values: Vec<UpValue>,
    call_frames: CallFrames,
    reg_stack: RegStack,
}

impl CallFrames {
    pub fn new(size: usize) -> CallFrames {
        let frames = vec![CallInfo{
            index: 0,
            func: 0,
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

    pub fn is_empty(&self) -> bool {
        self.pointer == 0
    }

    pub fn clear(&mut self) {
        self.pointer = 0;
    }

    pub fn push(&mut self, cf: CallInfo) {
        self.frames[self.pointer] = cf;
        self.frames[self.pointer].index = self.pointer;
        self.pointer += 1;
    }

    pub fn pop(&mut self) -> &mut CallInfo {
        self.pointer -= 1;
        &mut self.frames[self.pointer]
    }

    pub fn get(&self, index: usize) -> &CallInfo {
        &self.frames[index]
    }

    pub fn remove_at(&mut self, index: usize) {
        for i in index..self.pointer-1 {
            self.frames[i] = self.frames[i+1].clone();
            self.frames[i].index = i;
            self.pointer = i;
        }
        self.pointer += 1;
    }

    pub fn try_last(&self) -> Option<&CallInfo> {
        if self.pointer > 0 {
            Some(&self.frames[self.pointer-1])
        } else {
            None
        }
    }

    pub fn pointer(&self) -> usize {
        self.pointer
    }

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

    pub fn top(&self) -> usize {
        self.top
    }

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

    pub fn get(&self) -> &GosValue {
        &self.stack[self.top] 
    }

    pub fn set(&mut self, reg: usize, v: GosValue) {
        self.stack[reg] = v;
        self.top = reg + 1;
    }

    pub fn push(&mut self, v: GosValue) {
        self.stack[self.top] = v;
        self.top += 1;
    }

    pub fn pop(&mut self) -> GosValue {
        let v = self.stack[self.top].clone();
        self.stack[self.top] = GosValue::Nil;
        v
    }
    
    pub fn fill_nil(&mut self, start: usize, count: usize) {
        for i in start..start+count {
            self.stack[i] = GosValue::Nil;
        }
        self.top = start + count;
    }
}