use futures_lite::future;
use goscript_vm::ffi::{Ffi, FfiCtorResult};
use goscript_vm::value::{GosValue, RtMultiValResult};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

struct MutexApi {}

impl Ffi for MutexApi {
    fn call(
        &self,
        func_name: &str,
        params: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RtMultiValResult> + '_>> {
        let re = match func_name {
            "new" => MutexApi::new(params),
            "lock" => MutexApi::lock(params),
            "unlock" => MutexApi::unlock(params),
            _ => unreachable!(),
        };
        Box::pin(async move { re })
    }
}

impl MutexApi {
    fn new(params: Vec<GosValue>) -> RtMultiValResult {
        Ok(vec![])
    }

    fn lock(params: Vec<GosValue>) -> RtMultiValResult {
        Ok(vec![])
    }

    fn unlock(params: Vec<GosValue>) -> RtMultiValResult {
        Ok(vec![])
    }
}

struct Mutex {
    locked: Cell<bool>,
}

impl Mutex {
    fn new() -> Mutex {
        Mutex {
            locked: Cell::new(false),
        }
    }

    async fn lock(&self) -> RtMultiValResult {
        while self.locked.get() {
            future::yield_now().await;
        }
        self.locked.set(true);
        Ok(vec![])
    }

    async fn unlock(&self) -> RtMultiValResult {
        if !self.locked.get() {
            Err("sync: unlock of unlocked mutex".to_string())
        } else {
            self.locked.set(false);
            Ok(vec![])
        }
    }
}
