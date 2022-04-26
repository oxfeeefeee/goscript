// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

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
                .downcast_ref::<$typ>()
                .unwrap()
                .clone())
        }
    }};
}

#[derive(Ffi)]
pub struct MutexFfi {}

#[ffi_impl(rename = "sync.mutex")]
impl MutexFfi {
    pub fn new(_v: Vec<GosValue>) -> MutexFfi {
        MutexFfi {}
    }

    fn ffi_lock(
        &self,
        ctx: &mut FfiCallCtx,
        args: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Vec<GosValue>>> + '_>> {
        // It'd probably be cleaner if we use interface{} instead of pointer as
        // the argument, but let's leave it like this to serve as an example.
        match MutexFfi::create_mutex(&args[0], ctx) {
            Ok(mutex) => Box::pin((|| async move { mutex.lock().await })()),
            Err(e) => Box::pin(async move { Err(e) }),
        }
    }

    async fn ffi_unlock(&self, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let ud = args[0].as_unsafe_ptr();
        let mutex = ud.unwrap().downcast_ref::<Mutex>().unwrap().clone();
        mutex.unlock().await
    }

    fn create_mutex(arg: &GosValue, ctx: &mut FfiCallCtx) -> RuntimeResult<Mutex> {
        create_mutex!(arg, ctx, Mutex)
    }
}

#[derive(Clone)]
struct Mutex {
    locked: Rc<Cell<bool>>,
}

impl UnsafePtr for Mutex {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Mutex {
    fn new() -> Mutex {
        Mutex {
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
pub struct RWMutexFfi {}

#[ffi_impl(rename = "sync.rw_mutex")]
impl RWMutexFfi {
    pub fn new(_v: Vec<GosValue>) -> RWMutexFfi {
        RWMutexFfi {}
    }

    fn ffi_r_lock(
        &self,
        ctx: &mut FfiCallCtx,
        args: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Vec<GosValue>>> + '_>> {
        match RWMutexFfi::create_mutex(&args[0], ctx) {
            Ok(m) => Box::pin((|| async move { m.r_lock().await })()),
            Err(e) => Box::pin(async move { Err(e) }),
        }
    }

    async fn ffi_r_unlock(&self, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let ud = args[0].as_unsafe_ptr();
        let mutex = ud.unwrap().downcast_ref::<RWMutex>().unwrap().clone();
        mutex.r_unlock().await
    }

    fn ffi_w_lock(
        &self,
        ctx: &mut FfiCallCtx,
        args: Vec<GosValue>,
    ) -> Pin<Box<dyn Future<Output = RuntimeResult<Vec<GosValue>>> + '_>> {
        match RWMutexFfi::create_mutex(&args[0], ctx) {
            Ok(m) => Box::pin((|| async move { m.w_lock().await })()),
            Err(e) => Box::pin(async move { Err(e) }),
        }
    }

    async fn ffi_w_unlock(&self, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let ud = args[0].as_unsafe_ptr();
        let mutex = ud.unwrap().downcast_ref::<RWMutex>().unwrap().clone();
        mutex.w_unlock().await
    }

    fn create_mutex(arg: &GosValue, ctx: &mut FfiCallCtx) -> RuntimeResult<RWMutex> {
        create_mutex!(arg, ctx, RWMutex)
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
pub struct RWMutex {
    data: Rc<RefCell<RWMutexData>>,
}

impl UnsafePtr for RWMutex {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl RWMutex {
    fn new() -> RWMutex {
        RWMutex {
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
