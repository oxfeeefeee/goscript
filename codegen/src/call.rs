/// Helper for patching function calls
/// When the code of fuction calls are generated, the callee may not be ready yet,
/// we need to path them after codegen is all done.
///
///
use goscript_vm::instruction::{Instruction, OpIndex};
use goscript_vm::metadata::Meta;
use goscript_vm::value::{key_to_u64, FunctionKey, VMObjects};

struct CallPoint {
    func: FunctionKey,
    point: usize,
    meta: Meta,
    index: OpIndex,
}

pub struct CallHelper {
    calls: Vec<CallPoint>,
}

impl CallHelper {
    pub fn new() -> CallHelper {
        CallHelper { calls: vec![] }
    }

    pub fn add_call(&mut self, func: FunctionKey, point: usize, meta: Meta, index: OpIndex) {
        let p = CallPoint {
            func,
            point,
            meta,
            index,
        };
        self.calls.push(p);
    }

    pub fn patch_call(&self, objs: &mut VMObjects) {
        for call in self.calls.iter() {
            let method = call.meta.get_method(call.index, &objs.metas);
            let key = method.borrow().func.unwrap();
            let func = &mut objs.functions[call.func];
            *func.instruction_mut(call.point) = Instruction::from_u64(key_to_u64(key));
        }
    }
}
