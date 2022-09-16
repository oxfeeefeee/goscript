// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

extern crate self as goscript_engine;
use crate::ffi::*;
use goscript_vm::value::*;
use std::any::Any;
use std::cell::RefCell;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;

// Flags to OpenFile
const O_RDONLY: usize = 0x00000;
const O_WRONLY: usize = 0x00001;
const O_RDWR: usize = 0x00002;
const O_APPEND: usize = 0x00400;
const O_CREATE: usize = 0x00040;
const O_EXCL: usize = 0x00080;
const O_TRUNC: usize = 0x00200;

lazy_static! {
    static ref STD_IO_API: Arc<Mutex<StdIoApi>> = Arc::new(Mutex::new(StdIoApi::default()));
}

pub fn set_std_io(
    std_in: Option<Box<dyn std::io::Read + Sync + Send>>,
    std_out: Option<Box<dyn std::io::Write + Sync + Send>>,
    std_err: Option<Box<dyn std::io::Write + Sync + Send>>,
) {
    let mut api = STD_IO_API.lock().unwrap();
    api.std_in = std_in;
    api.std_out = std_out;
    api.std_err = std_err;
}

#[derive(Default)]
pub struct StdIoApi {
    pub(crate) std_in: Option<Box<dyn io::Read + Sync + Send>>,
    pub(crate) std_out: Option<Box<dyn io::Write + Sync + Send>>,
    pub(crate) std_err: Option<Box<dyn io::Write + Sync + Send>>,
}

#[derive(Ffi)]
pub struct FileFfi;

#[ffi_impl(rename = "os.file")]
impl FileFfi {
    fn ffi_get_std_io(i: isize) -> GosValue {
        match i {
            0 => VirtualFile::with_std_io(StdIo::StdIn).into_val(),
            1 => VirtualFile::with_std_io(StdIo::StdOut).into_val(),
            2 => VirtualFile::with_std_io(StdIo::StdErr).into_val(),
            _ => unreachable!(),
        }
    }

    fn ffi_open(path: GosValue, flags: isize) -> (GosValue, isize, GosValue) {
        let path = StrUtil::as_str(path.as_string());
        let flags = flags as usize;
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
            Some(f) => VirtualFile::with_sys_file(f).into_val(),
            None => FfiCtx::new_nil(ValueType::UnsafePtr),
        })
    }

    fn ffi_read(fp: GosValue, buffer: GosValue) -> RuntimeResult<(isize, isize, GosValue)> {
        let file = fp.as_non_nil_unsafe_ptr()?.downcast_ref::<VirtualFile>()?;
        let slice = &buffer.as_non_nil_slice::<Elem8>()?.0;
        let mut buf = unsafe { slice.as_raw_slice_mut::<u8>() };
        let r = file.read(&mut buf);
        Ok(FileFfi::result_to_go(r, |opt| opt.unwrap_or(0) as isize))
    }

    fn ffi_write(fp: GosValue, buffer: GosValue) -> RuntimeResult<(isize, isize, GosValue)> {
        let file = fp.as_non_nil_unsafe_ptr()?.downcast_ref::<VirtualFile>()?;
        let slice = &buffer.as_non_nil_slice::<Elem8>()?.0;
        let buf = unsafe { slice.as_raw_slice::<u8>() };
        let r = file.write(&buf);
        Ok(FileFfi::result_to_go(r, |opt| opt.unwrap_or(0) as isize))
    }

    fn ffi_seek(fp: GosValue, offset: i64, whence: isize) -> RuntimeResult<(i64, isize, GosValue)> {
        let file = fp.as_non_nil_unsafe_ptr()?.downcast_ref::<VirtualFile>()?;
        let whence = match whence {
            0 => io::SeekFrom::Start(offset as u64),
            1 => io::SeekFrom::Current(offset),
            2 => io::SeekFrom::End(offset),
            _ => unreachable!(),
        };
        let r = file.seek(whence);
        Ok(FileFfi::result_to_go(r, |opt| opt.unwrap_or(0) as i64))
    }

    fn result_to_go<IN, OUT, F>(result: io::Result<IN>, f: F) -> (OUT, isize, GosValue)
    where
        F: Fn(Option<IN>) -> OUT,
    {
        match result {
            Ok(i) => (f(Some(i)), 0, FfiCtx::new_string("")),
            Err(e) => (
                f(None),
                e.kind() as isize,
                FfiCtx::new_string(&e.to_string()),
            ),
        }
    }
}

pub enum StdIo {
    StdIn,
    StdOut,
    StdErr,
}

impl StdIo {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut api = STD_IO_API.lock().unwrap();
        match self {
            Self::StdIn => match &mut api.std_in {
                Some(r) => r.read(buf),
                None => io::stdin().lock().read(buf),
            },
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
        let mut api = STD_IO_API.lock().unwrap();
        match self {
            Self::StdOut => match &mut api.std_out {
                Some(r) => r.write(buf),
                None => io::stdout().lock().write(buf),
            },
            Self::StdErr => match &mut api.std_err {
                Some(r) => r.write(buf),
                None => io::stderr().lock().write(buf),
            },
            Self::StdIn => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "write to std in",
            )),
        }
    }
}

#[derive(UnsafePtr)]
pub enum VirtualFile {
    File(Rc<RefCell<fs::File>>),
    StdIo(StdIo),
}

impl VirtualFile {
    fn with_sys_file(f: fs::File) -> VirtualFile {
        VirtualFile::File(Rc::new(RefCell::new(f)))
    }

    fn with_std_io(io: StdIo) -> VirtualFile {
        VirtualFile::StdIo(io)
    }

    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::File(f) => f.borrow_mut().read(buf),
            Self::StdIo(io) => io.read(buf),
        }
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::File(f) => f.borrow_mut().write(buf),
            Self::StdIo(io) => io.write(buf),
        }
    }

    fn seek(&self, pos: io::SeekFrom) -> io::Result<u64> {
        match self {
            Self::File(f) => f.borrow_mut().seek(pos),
            Self::StdIo(_) => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "seek from std io",
            )),
        }
    }

    fn into_val(self) -> GosValue {
        FfiCtx::new_unsafe_ptr(self)
    }
}
