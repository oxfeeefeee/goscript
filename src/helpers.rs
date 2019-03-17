pub struct Defer<F: FnMut()> {
    f: F
}

impl <F: FnMut()> Defer<F> {
    pub fn new(func: F) -> Defer<F> {
        Defer{f: func}
    }
}

impl <F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.f)()
    }
}
