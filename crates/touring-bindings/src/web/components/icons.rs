//! Inline SVG icon set — Elite style (SPEC 2026-06-12 §3.5).
//!
//! 16px, stroke 1.5, linear/discrete glyphs replacing the unicode
//! sidebar markers. Markup strings are static so `icon_markup` stays
//! unit-testable without a DOM.

use leptos::prelude::*;

/// Inner SVG markup (24×24 viewBox) for a named icon. Unknown names
/// fall back to a neutral dot so a typo never renders an empty box.
pub fn icon_markup(name: &'static str) -> &'static str {
    match name {
        // Navigation
        "dashboard" => {
            r#"<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>"#
        }
        "quality" => {
            r#"<path d="M12 3l2.5 6 6.5.5-5 4.5 1.5 6.5-5.5-3.5-5.5 3.5L8 14 3 9.5l6.5-.5z"/>"#
        }
        "rules" => {
            r#"<path d="M12 3v18"/><path d="M5 7l7-3 7 3"/><path d="M5 7l-2 6h4z"/><path d="M19 7l-2 6h4z"/>"#
        }
        "diff" => r#"<path d="M12 4l8 16H4z"/>"#,
        "federation" => {
            r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a14 14 0 010 18a14 14 0 010-18"/>"#
        }
        "workspace" => {
            r#"<circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="9" ry="3.6"/>"#
        }
        "wiring" => {
            r#"<path d="M9 7V3M15 7V3"/><path d="M7 7h10v3a5 5 0 01-10 0z"/><path d="M12 15v6"/>"#
        }
        "chains" => {
            r#"<path d="M10 14a4 4 0 005.7 0l3-3a4 4 0 00-5.7-5.7l-1.2 1.2"/><path d="M14 10a4 4 0 00-5.7 0l-3 3a4 4 0 005.7 5.7l1.2-1.2"/>"#
        }
        "orphans" => {
            r#"<circle cx="12" cy="12" r="8" stroke-dasharray="4 3"/><circle cx="12" cy="12" r="1.6"/>"#
        }
        "search" => r#"<circle cx="11" cy="11" r="7"/><path d="M16 16l5 5"/>"#,
        "memory" => {
            r#"<path d="M4 7l8-4 8 4v10l-8 4-8-4z"/><path d="M4 7l8 4 8-4"/><path d="M12 11v10"/>"#
        }
        "plans" => {
            r#"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/><path d="M9 9h12"/><path d="M9 15h12"/>"#
        }
        "sessions" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3.5 2"/>"#,
        "cognitive" => {
            r#"<path d="M12 3a6 6 0 016 6c0 2.2-1 3.6-2 4.8-.8 1-1 1.8-1 3.2h-6c0-1.4-.2-2.2-1-3.2-1-1.2-2-2.6-2-4.8a6 6 0 016-6z"/><path d="M9.5 21h5"/>"#
        }
        "health" => r#"<path d="M3 12h4l2-6 4 12 2-6h6"/>"#,
        "hooks" => r#"<path d="M13 2L4 14h6l-1 8 9-12h-6z"/>"#,
        "settings" => {
            r#"<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M19.1 4.9L17 7M7 17l-2.1 2.1"/>"#
        }
        // Wave 4+ surfaces
        "mcp" => {
            r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 9l3 3-3 3"/><path d="M12 15h5"/>"#
        }
        "impact" => {
            r#"<circle cx="12" cy="12" r="2"/><circle cx="12" cy="12" r="5.5"/><circle cx="12" cy="12" r="9"/>"#
        }
        "speculate" => {
            r#"<path d="M10 3h4v5l4.5 8a3 3 0 01-2.6 4.5H8.1A3 3 0 015.5 16L10 8z"/><path d="M7.5 13h9"/>"#
        }
        "inspector" => {
            r#"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18M15 3v18"/>"#
        }
        // Chrome
        "bell" => {
            r#"<path d="M6 9a6 6 0 0112 0c0 5 2 6 2 6H4s2-1 2-6"/><path d="M10.5 19a2 2 0 003 0"/>"#
        }
        "command" => {
            r#"<path d="M9 9V6a3 3 0 10-3 3zM9 15v3a3 3 0 11-3-3zM15 9h3a3 3 0 10-3-3zM15 15h3a3 3 0 11-3 3zM9 9h6v6H9z"/>"#
        }
        "arrow-right" => r#"<path d="M5 12h14M13 6l6 6-6 6"/>"#,
        "external" => {
            r#"<path d="M14 4h6v6"/><path d="M20 4L10 14"/><path d="M9 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-3"/>"#
        }
        "page" => r#"<path d="M6 2h9l5 5v15H6z"/><path d="M15 2v5h5"/>"#,
        _ => r#"<circle cx="12" cy="12" r="2.5"/>"#,
    }
}

/// Elite inline icon — 16px, stroke 1.5, `currentColor`.
#[component]
pub fn Icon(
    /// Icon name (see [`icon_markup`] for the catalog).
    name: &'static str,
) -> impl IntoView {
    view! {
        <svg
            class="el-icon"
            viewBox="0 0 24 24"
            aria-hidden="true"
            inner_html=icon_markup(name)
        ></svg>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_nav_icon_has_markup() {
        let names = [
            "dashboard",
            "quality",
            "rules",
            "diff",
            "federation",
            "workspace",
            "wiring",
            "chains",
            "orphans",
            "search",
            "memory",
            "plans",
            "sessions",
            "cognitive",
            "health",
            "hooks",
            "settings",
            "mcp",
            "impact",
            "speculate",
            "inspector",
            "bell",
            "command",
            "arrow-right",
            "external",
            "page",
        ];
        for n in names {
            let m = icon_markup(n);
            assert!(m.contains("<"), "{n} markup must contain SVG elements");
            assert_ne!(
                m,
                icon_markup("__unknown__"),
                "{n} must not silently hit the fallback glyph"
            );
        }
    }

    #[test]
    fn unknown_icon_falls_back_to_dot() {
        assert!(icon_markup("definitely-not-an-icon").contains("circle"));
    }
}
