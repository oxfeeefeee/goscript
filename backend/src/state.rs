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

pub struct CallFrame {
}

pub struct CallFrameStack {
    frames: Vec<CallFrame>,
    sp: isize,
}

pub struct Registry {
    vec: Vec<GosValue>,
    top: isize,
}

pub struct State<'a> {
    g_state:    &'static GlobalState,
    parent:     Option<&'a State<'a>>,
    env:        HashMap<GosValue, GosValue>,
    up_values:  Vec<UpValue>,
    call_stack: CallFrameStack,
    reg_stack:  Registry,
}