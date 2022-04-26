// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::value::*;
use std::any::Any;
use std::cell::RefCell;
use std::fs;
use std::future::Future;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::pin::Pin;
use std::rc::Rc;

// Flags to OpenFile
const O_RDONLY: usize = 0x00000;
const O_WRONLY: usize = 0x00001;
const O_RDWR: usize = 0x00002;
const O_APPEND: usize = 0x00400;
const O_CREATE: usize = 0x00040;
const O_EXCL: usize = 0x00080;
const O_TRUNC: usize = 0x00200;

#[derive(Ffi)]
pub struct FileFfi {}

#[ffi_impl(rename = "os.file")]
impl FileFfi {
    pub fn new(_v: Vec<GosValue>) -> FileFfi {
        FileFfi {}
    }

    fn ffi_get_std_io(&self, args: Vec<GosValue>) -> GosValue {
        match *args[0].as_int() {
            0 => VirtualFile::StdIn.into_val(),
            1 => VirtualFile::StdOut.into_val(),
            2 => VirtualFile::StdErr.into_val(),
            _ => unreachable!(),
        }
    }

    fn ffi_open(&self, args: Vec<GosValue>) -> Vec<GosValue> {
        let path = StrUtil::as_str(args[0].as_string());
        let flags = *args[1].as_int() as usize;
        let mut options = fs::OpenOptions::new();
        match flags & O_RDWR {
            O_RDONLY => options.read(true),
            O_WRONLY => options.write(true),
            O_RDWR => options.read(true).write(true),
            _ => unreachable!(),
        };
        options.append((flags & O_APPEND) != 0);
        options.append((flags & O_TRUNC) != 0);
        match (((flags & O_CREATE) != 0), ((flags & O_EXCL) != 0)) {
            (true, false) => options.create(true),
            (true, true) => options.create_new(true),
            _ => &options,
        };
        let r = options.open(&*path);
        FileFfi::result_to_go(r, |opt| match opt {
            Some(f) => VirtualFile::File(Rc::new(RefCell::new(f))).into_val(),
            None => GosValue::new_nil(ValueType::UnsafePtr),
        })
    }

    fn ffi_read(&self, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let file = args[0]
            .as_some_unsafe_ptr()?
            .downcast_ref::<VirtualFile>()?;
        let slice = &args[1].as_some_slice::<Elem8>()?.0;
        let mut buf = unsafe { slice.as_raw_slice_mut::<u8>() };
        let r = file.read(&mut buf);
        Ok(FileFfi::result_to_go(r, |opt| {
            GosValue::new_int(opt.unwrap_or(0) as isize)
        }))
    }

    fn ffi_write(&self, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let file = args[0]
            .as_some_unsafe_ptr()?
            .downcast_ref::<VirtualFile>()?;
        let slice = &args[1].as_some_slice::<Elem8>()?.0;
        let buf = unsafe { slice.as_raw_slice::<u8>() };
        let r = file.write(&buf);
        Ok(FileFfi::result_to_go(r, |opt| {
            GosValue::new_int(opt.unwrap_or(0) as isize)
        }))
    }

    fn ffi_seek(&self, args: Vec<GosValue>) -> RuntimeResult<Vec<GosValue>> {
        let file = args[0]
            .as_some_unsafe_ptr()?
            .downcast_ref::<VirtualFile>()?;
        let offset = *args[1].as_int64();
        let whence = match *args[2].as_int() {
            0 => io::SeekFrom::Start(offset as u64),
            1 => io::SeekFrom::Current(offset),
            2 => io::SeekFrom::End(offset),
            _ => unreachable!(),
        };
        let r = file.seek(whence);
        Ok(FileFfi::result_to_go(r, |opt| {
            GosValue::new_uint64(opt.unwrap_or(0))
        }))
    }

    fn result_to_go<T, F>(result: io::Result<T>, f: F) -> Vec<GosValue>
    where
        F: Fn(Option<T>) -> GosValue,
    {
        let r = match result {
            Ok(i) => (f(Some(i)), 0, GosValue::with_str("")),
            Err(e) => (
                f(None),
                e.kind() as isize,
                GosValue::with_str(&e.to_string()),
            ),
        };
        vec![r.0, GosValue::new_int(r.1), r.2]
    }
}

#[derive(UnsafePtr)]
pub enum VirtualFile {
    File(Rc<RefCell<fs::File>>),
    StdIn,
    StdOut,
    StdErr,
}

impl VirtualFile {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::File(f) => f.borrow_mut().read(buf),
            Self::StdIn => io::stdin().lock().read(buf),
            Self::StdOut => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "read from std out",
            )),
            Self::StdErr => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "read from std error",
            )),
        }
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::File(f) => f.borrow_mut().write(buf),
            Self::StdOut => io::stdout().lock().write(buf),
            Self::StdErr => io::stderr().lock().write(buf),
            Self::StdIn => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "read from std in",
            )),
        }
    }

    fn seek(&self, pos: io::SeekFrom) -> io::Result<u64> {
        match self {
            Self::File(f) => f.borrow_mut().seek(pos),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "seek from std io",
            )),
        }
    }

    fn into_val(self) -> GosValue {
        GosValue::new_unsafe_ptr(self)
    }
}
