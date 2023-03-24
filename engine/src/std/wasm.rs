use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    // Use `js_namespace` here to bind `console.log(..)` instead of just
    // `log(..)`
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/// Logs a message to the console via wasm_bindgen.
pub(crate) fn console_log(s: &str) {
    log(s);
}
