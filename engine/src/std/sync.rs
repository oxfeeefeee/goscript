///https://en.wikipedia.org/wiki/Readers%E2%80%93writer_lock
///
///
extern crate self as goscript_engine;
use crate::ffi::*;
use futures_lite::future;
use goscript_vm::value::{GosValue, RuntimeResult, UnsafePtr};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::vec;

macro_rules! create_mutex {
    ($arg0:expr, $ctx:expr, $typ:tt) => {{
        let pp = $arg0.as_pointer().unwrap();
        let p = pp.deref(&$ctx.stack, &$ctx.vm_objs.packages)?;
        if p.is_nil() {
            let inner = $typ::new();
            let p = GosValue::new_unsafe_ptr(inner.clone());
            pp.set_pointee(&p, $ctx.stack, &$ctx.vm_objs.packages, &$ctx.gcv)?;
            Ok(inner)
        } else {
            Ok(p.as_unsafe_ptr()
                .unwrap()
                .as_any()
                .downcast_ref::<$typ>()
                .unwrap()
                .clone())
        }
    }};
}

#[derive(Ffi)]
pub struct Mutex {}

#[ffi_impl(rename = "sync.mutex")]
impl Mutex {
    pub fn new(_v: Vec<GosValue>) -> Mutex {
        Mutex {}
    }

    fn ffi_lock(
        &self,
        ctx: &mut FfiCallCtx,
        args: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Vec<GosValue>>> + '_>> {
        // It'd probably be cleaner if we use interface{} instead of pointer as
        // the argument, but let's leave it like this to serve as an example.
        match Mutex::create_mutex(&args[0], ctx) {
            Ok(mutex) => Box::pin((|| async move { mutex.lock().await })()),
            Err(e) => Box::pin(async move { Err(e) }),
        }
    }

    async fn ffi_unlock(&self, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let ud = args[0].as_unsafe_ptr();
        let mutex = ud
            .unwrap()
            .as_any()
            .downcast_ref::<MutexInner>()
            .unwrap()
            .clone();
        mutex.unlock().await
    }

    fn create_mutex(arg: &GosValue, ctx: &mut FfiCallCtx) -> RuntimeResult<MutexInner> {
        create_mutex!(arg, ctx, MutexInner)
    }
}

#[derive(Clone)]
struct MutexInner {
    locked: Rc<Cell<bool>>,
}

impl UnsafePtr for MutexInner {
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

    async fn lock(self) -> RuntimeResult<Vec<GosValue>> {
        //dbg!("lock called");
        while self.locked.get() {
            future::yield_now().await;
        }
        self.locked.set(true);
        Ok(vec![])
    }

    async fn unlock(self) -> RuntimeResult<Vec<GosValue>> {
        //dbg!("unlock called");
        if !self.locked.get() {
            Err("sync: unlock of unlocked mutex".to_owned())
        } else {
            self.locked.set(false);
            Ok(vec![])
        }
    }
}

#[derive(Ffi)]
pub struct RWMutex {}

#[ffi_impl(rename = "sync.rw_mutex")]
impl RWMutex {
    pub fn new(_v: Vec<GosValue>) -> RWMutex {
        RWMutex {}
    }

    fn ffi_r_lock(
        &self,
        ctx: &mut FfiCallCtx,
        args: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Vec<GosValue>>> + '_>> {
        match RWMutex::create_mutex(&args[0], ctx) {
            Ok(m) => Box::pin((|| async move { m.r_lock().await })()),
            Err(e) => Box::pin(async move { Err(e) }),
        }
    }

    async fn ffi_r_unlock(&self, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let ud = args[0].as_unsafe_ptr();
        let mutex = ud
            .unwrap()
            .as_any()
            .downcast_ref::<RWMutexInner>()
            .unwrap()
            .clone();
        mutex.r_unlock().await
    }

    fn ffi_w_lock(
        &self,
        ctx: &mut FfiCallCtx,
        args: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Vec<GosValue>>> + '_>> {
        match RWMutex::create_mutex(&args[0], ctx) {
            Ok(m) => Box::pin((|| async move { m.w_lock().await })()),
            Err(e) => Box::pin(async move { Err(e) }),
        }
    }

    async fn ffi_w_unlock(&self, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let ud = args[0].as_unsafe_ptr();
        let mutex = ud
            .unwrap()
            .as_any()
            .downcast_ref::<RWMutexInner>()
            .unwrap()
            .clone();
        mutex.w_unlock().await
    }

    fn create_mutex(arg: &GosValue, ctx: &mut FfiCallCtx) -> RuntimeResult<RWMutexInner> {
        create_mutex!(arg, ctx, RWMutexInner)
    }
}

pub struct RWMutexData {
    num_writers_waiting: isize,
    writer_active: bool,
    num_readers_active: isize,
}

impl RWMutexData {
    fn new() -> RWMutexData {
        RWMutexData {
            num_writers_waiting: 0,
            writer_active: false,
            num_readers_active: 0,
        }
    }

    fn can_read(&self) -> bool {
        !self.writer_active && self.num_writers_waiting == 0
    }

    fn can_write(&self) -> bool {
        !self.writer_active && self.num_readers_active == 0
    }

    fn inc_reader_num(&mut self) -> isize {
        self.num_readers_active += 1;
        self.num_readers_active
    }

    fn dec_reader_num(&mut self) -> isize {
        self.num_readers_active -= 1;
        self.num_readers_active
    }

    fn inc_writer_num(&mut self) -> isize {
        self.num_writers_waiting += 1;
        self.num_writers_waiting
    }

    fn dec_writer_num(&mut self) -> isize {
        self.num_writers_waiting -= 1;
        self.num_writers_waiting
    }

    fn set_writer_active(&mut self, val: bool) -> bool {
        let old_val = self.writer_active;
        self.writer_active = val;
        old_val
    }
}

#[derive(Clone)]
pub struct RWMutexInner {
    data: Rc<RefCell<RWMutexData>>,
}

impl UnsafePtr for RWMutexInner {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl RWMutexInner {
    fn new() -> RWMutexInner {
        RWMutexInner {
            data: Rc::new(RefCell::new(RWMutexData::new())),
        }
    }

    async fn r_lock(self) -> RuntimeResult<Vec<GosValue>> {
        loop {
            let can_read = self.data.borrow().can_read();
            if !can_read {
                future::yield_now().await;
            } else {
                break;
            }
        }
        self.data.borrow_mut().inc_reader_num();
        Ok(vec![])
    }

    async fn r_unlock(self) -> RuntimeResult<Vec<GosValue>> {
        let num = self.data.borrow_mut().dec_reader_num();
        if num < 0 {
            Err("sync: unmatched rUnlock call".to_owned())
        } else {
            Ok(vec![])
        }
    }

    async fn w_lock(self) -> RuntimeResult<Vec<GosValue>> {
        self.data.borrow_mut().inc_writer_num();
        loop {
            let can_write = self.data.borrow().can_write();
            if !can_write {
                future::yield_now().await;
            } else {
                break;
            }
        }
        let mut d = self.data.borrow_mut();
        d.dec_writer_num();
        d.set_writer_active(true);
        Ok(vec![])
    }

    async fn w_unlock(self) -> RuntimeResult<Vec<GosValue>> {
        let was_active = self.data.borrow_mut().set_writer_active(false);
        if !was_active {
            Err("sync: unmatched wUnlock call".to_owned())
        } else {
            Ok(vec![])
        }
    }
}
