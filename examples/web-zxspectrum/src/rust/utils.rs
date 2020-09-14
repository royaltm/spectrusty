/*
    web-zxspectrum: ZX Spectrum emulator example as a Web application.
    Copyright (C) 2020  Rafal Michalski

    For the full copyright notice, see the lib.rs file.
*/
#![macro_use]

// macro_rules! console_log {
//     ($($t:tt)*) => (crate::log(&format_args!($($t)*).to_string()))
// }

macro_rules! alert {
    ($($t:tt)*) => (crate::alert(&format_args!($($t)*).to_string()))
}

use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

pub type Result<T> = core::result::Result<T, JsValue>;

pub trait JsErr: Sized {
    type Ok;
    fn js_err(self) -> Result<Self::Ok>;
}

impl<T, E> JsErr for core::result::Result<T, E>
    where E: core::fmt::Display
{
    type Ok = T;
    fn js_err(self) -> Result<T> {
        self.map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

#[wasm_bindgen(js_name = setPanicHook)]
/// Call it once the wasm module has been loaded to register a panic hook.
pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();

    #[cfg(feature = "console_log")]
    {
        use log::Level;
        console_log::init_with_level(Level::Debug).expect("error initializing log");
    }
}

// relative to wasm-pack --out-dir, `pkg` by default.
#[wasm_bindgen(raw_module = "../src/js/utils")]
extern "C" {
    pub fn now() -> f64;
}

// pub fn now() -> f64 {
//     web_sys::window().unwrap().performance().unwrap().now()
// }
