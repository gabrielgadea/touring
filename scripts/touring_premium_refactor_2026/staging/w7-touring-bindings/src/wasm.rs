//! wasm-bindgen bindings (W7.4 placeholder).

#[cfg(feature = "bind-wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "bind-wasm")]
use crate::common::Greeting;

#[cfg(feature = "bind-wasm")]
#[wasm_bindgen]
pub fn hello(message: String) -> String {
    let g = Greeting::new(message);
    format!("{} (touring {})", g.message, g.touring_version)
}
