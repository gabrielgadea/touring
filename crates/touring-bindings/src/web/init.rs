//! WASM entry point — mounts the Leptos App to `#app`.

#[cfg(target_arch = "wasm32")]
use crate::web::app::App;
#[cfg(target_arch = "wasm32")]
use leptos::mount::mount_to;
#[cfg(target_arch = "wasm32")]
use leptos::view;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// WASM entry point — locates `#app` in the DOM and mounts the Leptos [`App`].
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_start() {
    let app: web_sys::HtmlElement = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document")
        .get_element_by_id("app")
        .expect("no #app element in index.html")
        .unchecked_into();
    web_sys::console::log_1(&"APP_ELEMENT_FOUND".into());
    std::mem::forget(mount_to(app, || view! { <App /> }));
    web_sys::console::log_1(&"APP_MOUNTED".into());
    // Debug: check what was mounted
    if let Some(body) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.body())
    {
        web_sys::console::log_1(&format!("body.child_count={}", body.child_element_count()).into());
        web_sys::console::log_1(&format!("body.inner_html len={}", body.inner_html().len()).into());
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// No-op on non-WASM targets (server-side rendering or native binary).
pub fn wasm_start() {}
