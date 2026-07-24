//! Theme system for touring-web.
//!
//! CSS variables for dark/light themes. The `Theme` enum carries the
//! runtime signal that drives the `data-theme` attribute on `<html>`.

use leptos::reactive::signal::RwSignal;
use serde::{Deserialize, Serialize};
use web_sys::wasm_bindgen::JsValue;

/// Storage key for persisting theme in localStorage.
const STORAGE_KEY: &str = "touring-web-theme";

/// Available theme variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    /// Dark theme variant.
    #[default]
    Dark,
    /// Light theme variant.
    Light,
}

impl Theme {
    /// Returns CSS variable file for this theme variant.
    pub fn css_vars(self) -> &'static str {
        // Single source of truth — the SAME file Trunk processes via
        // data-trunk rel="css" in touring-web/index.html (the elite design
        // system). Cross-audit 2026-06-11: this used to embed a stale legacy
        // stylesheet that overrode the elite design at runtime.
        match self {
            Theme::Dark => include_str!("../../../touring-web/public/assets/styles/main.css"),
            Theme::Light => include_str!("../../../touring-web/public/assets/styles/main.css"),
        }
    }

    /// Returns the string name used for localStorage key and data-theme attribute.
    pub fn name(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    /// Parse a theme from a string name, returning None for unrecognized values.
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            _ => None,
        }
    }
}

/// Load theme from localStorage on app mount.
/// Returns `Theme::Dark` as default if not set or on parse error.
pub fn load_theme() -> Theme {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return Theme::Dark,
    };
    let storage = match window.local_storage() {
        Ok(Some(s)) => s,
        _ => return Theme::Dark,
    };
    let stored = match storage.get_item(STORAGE_KEY) {
        Ok(Some(s)) => s,
        _ => return Theme::Dark,
    };
    Theme::from_name(&stored).unwrap_or(Theme::Dark)
}

/// Apply the given theme's CSS variables to the document root
/// AND persist to localStorage.
pub fn apply_theme(theme: Theme) {
    // Flip `data-theme` on <html> so [data-theme] selectors react on toggle
    // (boot does the same in `theme_signal()`; this keeps toggle consistent).
    // Cross-audit 2026-06-11: the old code looped over `--var` lines calling
    // `set_attribute("--x", …)` — attribute names cannot start with `--`, so
    // every call failed with a swallowed InvalidCharacterError (pure no-op).
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(html) = document.document_element() {
                let _ = html.set_attribute("data-theme", theme.name());
            }
        }
    }
    // Persist to localStorage.
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _: Result<(), JsValue> = storage.set_item(STORAGE_KEY, theme.name());
        }
    }
}

/// Global theme signal — toggling this updates the `data-theme` on `<html>`.
/// Initialized from localStorage via `load_theme()`.
pub fn theme_signal() -> RwSignal<Theme> {
    web_sys::console::log_1(&"LOADING_THEME_SIGNAL".into());
    let theme = load_theme();

    // CRITICAL: Set data-theme on <html> BEFORE injecting CSS.
    // CSS selectors like [data-theme="dark"] body rely on this attribute.
    // apply_theme() sets it too, but we need it BEFORE mount_to for first render.
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(html) = document.document_element() {
                let _ = html.set_attribute("data-theme", theme.name());
            }
        }
    }

    // Inject CSS at startup to avoid external stylesheet dependency.
    // Source: touring-web/public/assets/styles/main.css (the SAME file Trunk
    // hashes + serves via <link>) — single inclusion point in `css_vars()`.
    // The <style> tag we create lives in <head> AFTER the Trunk <link>, so it
    // wins the cascade — it MUST stay identical to what Trunk serves.
    let css = theme.css_vars();
    web_sys::console::log_1(&"INJECTING_CSS".into());
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            let style = document.create_element("style").ok();
            if let Some(style) = style {
                let _ = style.set_attribute("type", "text/css");
                let _ = style.set_attribute("id", "touring-web-main-styles");
                let _ = style.set_text_content(Some(css));
                let _ = document.head().map(|h| h.append_child(&style).ok());
            }
        }
    }
    web_sys::console::log_1(&"CSS_INJECTED".into());

    RwSignal::new(theme)
}
