use futures_lite::future;
use goscript_vm::ffi::{Ffi, FfiCtorResult};
use goscript_vm::value::{GosValue, PointerObj, RtMultiValResult, UserData};
use std::any::Any;
use std::cell::Cell;
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub struct Mutex {}

impl Ffi for Mutex {
    fn call(
        &self,
        func_name: &str,
        params: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RtMultiValResult> + '_>> {
        match func_name {
            "new" => {
                dbg!(params, "nnnnew");
                let p = PointerObj::UserData(Rc::new(MutexInner::new()));
                Box::pin(async move { Ok(vec![GosValue::new_pointer(p)]) })
            }
            "lock" => {
                dbg!(&params, "lllock");
                let ud = params[0].as_pointer().as_user_data();
                let mutex = ud.as_any().downcast_ref::<MutexInner>().unwrap().clone();
                Box::pin(mutex.lock())
            }
            "unlock" => {
                let ud = params[0].as_pointer().as_user_data();
                let mutex = ud.as_any().downcast_ref::<MutexInner>().unwrap().clone();
                Box::pin(mutex.unlock())
            }
            _ => unreachable!(),
        }
    }
}

impl Mutex {
    pub fn new(_v: Vec<GosValue>) -> FfiCtorResult<Rc<RefCell<dyn Ffi>>> {
        Ok(Rc::new(RefCell::new(Mutex {})))
    }
}

#[derive(Clone)]
struct MutexInner {
    locked: Rc<Cell<bool>>,
}

impl UserData for MutexInner {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl MutexInner {
    fn new() -> MutexInner {
        MutexInner {
            locked: Rc::new(Cell::new(false)),
        }
    }

    async fn lock(self) -> RtMultiValResult {
        dbg!("lock called");
        while self.locked.get() {
            future::yield_now().await;
        }
        self.locked.set(true);
        Ok(vec![])
    }

    async fn unlock(self) -> RtMultiValResult {
        if !self.locked.get() {
            Err("sync: unlock of unlocked mutex".to_string())
        } else {
            self.locked.set(false);
            Ok(vec![])
        }
    }
}
