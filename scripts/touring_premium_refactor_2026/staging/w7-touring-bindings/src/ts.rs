//! napi-rs TypeScript bindings (W7.3 placeholder).

#[cfg(feature = "bind-ts-napi")]
use napi_derive::napi;

#[cfg(feature = "bind-ts-napi")]
use crate::common::Greeting;

#[cfg(feature = "bind-ts-napi")]
#[napi]
pub fn hello(message: String) -> String {
    let g = Greeting::new(message);
    format!("{} (touring {})", g.message, g.touring_version)
}
