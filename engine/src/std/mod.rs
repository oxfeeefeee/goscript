mod bits;
mod fmt2;
mod io;
mod os;
mod reflect;
mod sync;

pub(crate) fn register(engine: &mut crate::engine::Engine) {
    fmt2::Fmt2Ffi::register(engine);
    bits::BitsFfi::register(engine);
    sync::MutexFfi::register(engine);
    sync::RWMutexFfi::register(engine);
    reflect::ReflectFfi::register(engine);
    io::IoFfi::register(engine);
    os::FileFfi::register(engine);
}
