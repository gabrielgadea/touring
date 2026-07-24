//! EliteShell — the single global wrapper (SPEC 2026-06-12 §4.1).
//!
//! Titlebar (44px: traffic lights · app · breadcrumb · ⌘K search ·
//! theme toggle) + body grid (232px sidebar | main) + the global
//! command palette overlay. Pages render inside `el-main` and never
//! instantiate their own sidebar/topbar again.

use crate::web::components::command_palette::CommandPalette;
use crate::web::components::icons::Icon;
use crate::web::components::sidebar::{NavItem, Sidebar};
use crate::web::components::theme_toggle::ThemeToggle;
use crate::web::ctx::{WorkspaceCtx, use_refresh_bus, use_workspace, workspace_label};
use crate::web::services::fetch_status;
use leptos::prelude::*;
use leptos_router::hooks::use_location;

/// Context wrapper for the global ⌘K palette open state.
#[derive(Clone, Copy)]
pub struct PaletteCtx(pub RwSignal<bool>);

/// Resolve the breadcrumb (section, page) labels for a pathname.
/// Exact matches come from [`NavItem`]; unknown routes echo the path.
pub fn breadcrumb_for(pathname: &str) -> (&'static str, String) {
    let path = if pathname.is_empty() || pathname == "/" {
        "/dashboard"
    } else {
        pathname
    };
    if let Some(item) = NavItem::iter().find(|i| i.route() == path) {
        return (item.section().label(), item.label().to_string());
    }
    // Nested/new routes: take the first segment's nav entry when present.
    let first = path.split('/').nth(1).unwrap_or_default();
    let parent = format!("/{first}");
    if let Some(item) = NavItem::iter().find(|i| i.route() == parent) {
        return (
            item.section().label(),
            format!(
                "{} · {}",
                item.label(),
                path.rsplit('/').next().unwrap_or("")
            ),
        );
    }
    ("Touring", path.trim_start_matches('/').to_string())
}

/// Global Elite shell — instantiated once in `App`.
#[component]
pub fn EliteShell(children: Children) -> impl IntoView {
    let palette_open = RwSignal::new(false);
    provide_context(PaletteCtx(palette_open));

    let workspace = use_workspace();
    let bus = use_refresh_bus();

    // Daemon status probe — refreshes on every bus tick.
    let status_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_status().await }
    });
    Effect::new(move |_| {
        if let Some(result) = status_res.get() {
            match result {
                Ok(v) => {
                    workspace
                        .daemon_status
                        .set(WorkspaceCtx::classify_status(&v));
                    let root = WorkspaceCtx::extract_root(&v);
                    if !root.is_empty() {
                        workspace.project_path.set(root);
                    }
                }
                Err(_) => workspace
                    .daemon_status
                    .set(crate::web::ctx::DaemonStatus::Down),
            }
        }
    });

    // Global ⌘K / Ctrl+K listener — registered client-side only.
    Effect::new(move |_| {
        let handle = window_event_listener(leptos::ev::keydown, move |e| {
            if (e.meta_key() || e.ctrl_key()) && e.key().eq_ignore_ascii_case("k") {
                e.prevent_default();
                palette_open.update(|o| *o = !*o);
            }
            if e.key() == "Escape" {
                palette_open.set(false);
            }
        });
        on_cleanup(move || handle.remove());
    });

    let location = use_location();
    let crumb = Memo::new(move |_| breadcrumb_for(&location.pathname.get()));
    let ws_label = Memo::new(move |_| workspace_label(&workspace.project_path.get()));

    view! {
        <div class="el-shell">
            <header class="el-titlebar">
                <div class="el-tl" aria-hidden="true"><span></span><span></span><span></span></div>
                <span class="el-titlebar-app">"Touring"</span>
                <div class="el-titlebar-div" aria-hidden="true"></div>
                <nav class="el-breadcrumb" aria-label="breadcrumb">
                    <span title=move || workspace.project_path.get()>{move || ws_label.get()}</span>
                    <span class="sep">"/"</span>
                    <span>{move || crumb.get().0}</span>
                    <span class="sep">"/"</span>
                    <span class="cur">{move || crumb.get().1}</span>
                </nav>
                <div class="el-titlebar-spacer"></div>
                <button
                    class="el-titlebar-search"
                    on:click=move |_| palette_open.set(true)
                    aria-label="open command palette"
                >
                    <Icon name="search"/>
                    <span>"Search or jump to…"</span>
                    <span class="el-kbd">"⌘K"</span>
                </button>
                <ThemeToggle/>
            </header>
            <div class="el-shell-body">
                <Sidebar/>
                <main class="el-main">{children()}</main>
            </div>
            <CommandPalette open=palette_open/>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumb_maps_known_routes() {
        assert_eq!(
            breadcrumb_for("/quality"),
            ("Sentrux", "Quality Detail".to_string())
        );
        assert_eq!(
            breadcrumb_for("/health"),
            ("Diagnostics", "Health".to_string())
        );
        assert_eq!(
            breadcrumb_for("/workspace"),
            ("Code Intelligence", "Workspace Atlas".to_string())
        );
    }

    #[test]
    fn breadcrumb_root_aliases_dashboard() {
        assert_eq!(breadcrumb_for("/"), breadcrumb_for("/dashboard"));
        assert_eq!(breadcrumb_for(""), breadcrumb_for("/dashboard"));
    }

    #[test]
    fn breadcrumb_nested_route_uses_parent_section() {
        let (section, label) = breadcrumb_for("/quality/rules");
        // Exact NavItem match exists for /quality/rules.
        assert_eq!(section, "Sentrux");
        assert!(label.contains("Rules"), "label was {label}");
    }

    #[test]
    fn breadcrumb_unknown_route_echoes_path() {
        let (section, label) = breadcrumb_for("/totally-new");
        assert_eq!(section, "Touring");
        assert_eq!(label, "totally-new");
    }
}
