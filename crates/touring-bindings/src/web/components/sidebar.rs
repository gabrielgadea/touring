//! Sidebar navigation component.
//!
//! Wave 4 P10 (2026-05-09) — split flat list into sectioned groups so
//! every dashboard route is reachable: Sentrux (5 routes) + Code
//! Intelligence (5 routes) + Diagnostics (1 route) = 11 entries total,
//! mirroring `app.rs` route registry exactly.

use leptos::prelude::*;

/// Navigation items available in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    // Sentrux — Wave 4 P10 dashboard routes.
    /// Unified Sentrux dashboard hero (`/dashboard`).
    Dashboard,
    /// Quality signal detail with radar chart (`/quality`).
    QualityDetail,
    /// TOML rule editor (`/quality/rules`).
    QualityRules,
    /// Snapshot diff comparator (`/quality/diff`).
    QualityDiff,
    /// Multi-workspace federation heatmap (`/federation`).
    Federation,
    /// 6-layer speculative validation radar (`/speculate`). Elite W4 (SPEC §6.3).
    Speculate,
    // Code Intelligence — wiring, search, structure.
    /// Workspace 3D Pauling-shell graph (`/workspace`).
    Workspace,
    /// Wiring graph viewer (`/wiring`).
    Wiring,
    /// BFS blast-radius explorer (`/wiring/impact`). Elite W4 (SPEC §6.2).
    WiringImpact,
    /// Functional chains Pauling diagram (`/chains`). Wave 4 2026-06-12.
    Chains,
    /// Orphan symbols list (`/orphans`).
    Orphans,
    /// BM25 symbol search (`/search`).
    Search,
    /// Memory recall (`/memory`).
    Memory,
    // Orchestration — DAGs, sessions, cognition. Wave 4 2026-06-12.
    /// Decompose plans board (`/plans`).
    Plans,
    /// Agent session runs (`/sessions`).
    Sessions,
    /// Cognitive engines health (`/cognitive`).
    Cognitive,
    /// Curated MCP tool executor (`/mcp`). Elite W4 (SPEC §6.1).
    Mcp,
    // Diagnostics — daemon health.
    /// Daemon health dashboard (`/health`).
    Health,
    /// Live hook/gate observability (`/hooks`). Wave 4 2026-06-12.
    HooksLive,
    /// Tri-pane library/canvas/inspector explorer (`/inspector`). W6 (SPEC §6.5).
    Inspector,
    // System.
    /// Theme + daemon settings (`/settings`). Wave 4 2026-06-12.
    Settings,
}

/// Logical grouping for the sidebar — used to render section headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavSection {
    /// Wave 4 P10 Sentrux quality views.
    Sentrux,
    /// Code intelligence (graphs, search, memory).
    CodeIntelligence,
    /// Task DAGs, sessions, cognitive engines (Wave 4 2026-06-12).
    Orchestration,
    /// Daemon + index diagnostics.
    Diagnostics,
    /// Theme + system settings (Wave 4 2026-06-12).
    System,
}

impl NavSection {
    /// Human-readable section header.
    pub fn label(self) -> &'static str {
        match self {
            NavSection::Sentrux => "Sentrux",
            NavSection::CodeIntelligence => "Code Intelligence",
            NavSection::Orchestration => "Orchestration",
            NavSection::Diagnostics => "Diagnostics",
            NavSection::System => "System",
        }
    }
}

impl NavItem {
    /// Returns the human-readable label for this navigation item.
    pub fn label(self) -> &'static str {
        match self {
            NavItem::Dashboard => "Dashboard",
            NavItem::QualityDetail => "Quality Detail",
            NavItem::QualityRules => "Quality Rules",
            NavItem::QualityDiff => "Quality Diff",
            NavItem::Federation => "Federation",
            NavItem::Speculate => "Speculate",
            NavItem::Workspace => "Workspace Atlas",
            NavItem::Wiring => "Wiring",
            NavItem::WiringImpact => "Impact",
            NavItem::Chains => "Chains",
            NavItem::Orphans => "Orphans",
            NavItem::Search => "Search",
            NavItem::Memory => "Memory",
            NavItem::Plans => "Plans",
            NavItem::Sessions => "Sessions",
            NavItem::Cognitive => "Cognitive",
            NavItem::Mcp => "MCP Tools",
            NavItem::Health => "Health",
            NavItem::HooksLive => "Hooks Live",
            NavItem::Inspector => "Inspector",
            NavItem::Settings => "Settings",
        }
    }

    /// Returns the URL path for this navigation item.
    pub fn route(self) -> &'static str {
        match self {
            NavItem::Dashboard => "/dashboard",
            NavItem::QualityDetail => "/quality",
            NavItem::QualityRules => "/quality/rules",
            NavItem::QualityDiff => "/quality/diff",
            NavItem::Federation => "/federation",
            NavItem::Speculate => "/speculate",
            NavItem::Workspace => "/workspace",
            NavItem::Wiring => "/wiring",
            NavItem::WiringImpact => "/wiring/impact",
            NavItem::Chains => "/chains",
            NavItem::Orphans => "/orphans",
            NavItem::Search => "/search",
            NavItem::Memory => "/memory",
            NavItem::Plans => "/plans",
            NavItem::Sessions => "/sessions",
            NavItem::Cognitive => "/cognitive",
            NavItem::Mcp => "/mcp",
            NavItem::Health => "/health",
            NavItem::HooksLive => "/hooks",
            NavItem::Inspector => "/inspector",
            NavItem::Settings => "/settings",
        }
    }

    /// SVG icon name for this entry (see `icons::icon_markup`).
    /// Elite W1 (SPEC §3.5): unicode glyphs replaced by inline SVG.
    pub fn icon(self) -> &'static str {
        match self {
            NavItem::Dashboard => "dashboard",
            NavItem::QualityDetail => "quality",
            NavItem::QualityRules => "rules",
            NavItem::QualityDiff => "diff",
            NavItem::Federation => "federation",
            NavItem::Speculate => "speculate",
            NavItem::Workspace => "workspace",
            NavItem::Wiring => "wiring",
            NavItem::WiringImpact => "impact",
            NavItem::Chains => "chains",
            NavItem::Orphans => "orphans",
            NavItem::Search => "search",
            NavItem::Memory => "memory",
            NavItem::Plans => "plans",
            NavItem::Sessions => "sessions",
            NavItem::Cognitive => "cognitive",
            NavItem::Mcp => "mcp",
            NavItem::Health => "health",
            NavItem::HooksLive => "hooks",
            NavItem::Inspector => "inspector",
            NavItem::Settings => "settings",
        }
    }

    /// Logical section this nav item belongs to.
    pub fn section(self) -> NavSection {
        match self {
            NavItem::Dashboard
            | NavItem::QualityDetail
            | NavItem::QualityRules
            | NavItem::QualityDiff
            | NavItem::Federation
            | NavItem::Speculate => NavSection::Sentrux,
            NavItem::Workspace
            | NavItem::Wiring
            | NavItem::WiringImpact
            | NavItem::Chains
            | NavItem::Orphans
            | NavItem::Search
            | NavItem::Memory => NavSection::CodeIntelligence,
            NavItem::Plans | NavItem::Sessions | NavItem::Cognitive | NavItem::Mcp => {
                NavSection::Orchestration
            }
            NavItem::Health | NavItem::HooksLive | NavItem::Inspector => NavSection::Diagnostics,
            NavItem::Settings => NavSection::System,
        }
    }

    /// Iterator over every nav item, in display order.
    pub fn iter() -> impl Iterator<Item = NavItem> {
        [
            // Sentrux — flagship dashboard surfaces first.
            NavItem::Dashboard,
            NavItem::QualityDetail,
            NavItem::QualityRules,
            NavItem::QualityDiff,
            NavItem::Federation,
            NavItem::Speculate,
            // Code Intelligence — exploration tools.
            NavItem::Workspace,
            NavItem::Wiring,
            NavItem::WiringImpact,
            NavItem::Chains,
            NavItem::Orphans,
            NavItem::Search,
            NavItem::Memory,
            // Orchestration — DAGs, sessions, cognition.
            NavItem::Plans,
            NavItem::Sessions,
            NavItem::Cognitive,
            NavItem::Mcp,
            // Diagnostics — daemon health + live hooks + tri-pane inspector.
            NavItem::Health,
            NavItem::HooksLive,
            NavItem::Inspector,
            // System.
            NavItem::Settings,
        ]
        .into_iter()
    }
}

/// Elite sidebar — 232px, `.el-side` items with SVG icons, section
/// eyebrows, and a footer carrying live daemon status + user card
/// (SPEC 2026-06-12 §4.1).
#[component]
pub fn Sidebar() -> impl IntoView {
    use crate::web::components::icons::Icon;
    use crate::web::ctx::use_workspace;
    use leptos_router::hooks::use_location;

    let location = use_location();
    let workspace = use_workspace();

    // Group items by section so we render section headers between groups.
    let grouped: Vec<(NavSection, Vec<NavItem>)> = {
        let mut acc: Vec<(NavSection, Vec<NavItem>)> = Vec::new();
        for item in NavItem::iter() {
            let sec = item.section();
            match acc.last_mut() {
                Some((s, v)) if *s == sec => v.push(item),
                _ => acc.push((sec, vec![item])),
            }
        }
        acc
    };

    view! {
        <nav class="el-sidebar" aria-label="primary">
            {grouped.into_iter().map(|(section, items)| {
                view! {
                    <div>
                        <div class="el-side-section">{section.label()}</div>
                        {items.into_iter().map(|item| {
                            let label = item.label();
                            let route = item.route();
                            let icon  = item.icon();
                            let is_active = move || {
                                let p = location.pathname.get();
                                let p = if p.is_empty() || p == "/" { "/dashboard".to_string() } else { p };
                                p == route
                            };
                            view! {
                                <a
                                    href=route
                                    class="el-side"
                                    class:active=is_active
                                    aria-label=label
                                >
                                    <Icon name=icon/>
                                    <span>{label}</span>
                                </a>
                            }
                        }).collect_view()}
                    </div>
                }
            }).collect_view()}

            <div class="el-sidebar-footer">
                <div class="el-daemon-status">
                    <span
                        class="el-pulse"
                        style=move || format!("background:{}", workspace.daemon_status.get().color())
                        aria-hidden="true"
                    ></span>
                    <span>{move || workspace.daemon_status.get().label()}</span>
                </div>
                <div class="el-user-card">
                    <div class="el-avatar" aria-hidden="true">"G"</div>
                    <span class="el-user-name">"Gabriel"</span>
                </div>
            </div>
        </nav>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_item_routes_match_app_router() {
        // Routes here MUST match the registrations in `crate::app::App`.
        assert_eq!(NavItem::Dashboard.route(), "/dashboard");
        assert_eq!(NavItem::QualityDetail.route(), "/quality");
        assert_eq!(NavItem::QualityRules.route(), "/quality/rules");
        assert_eq!(NavItem::QualityDiff.route(), "/quality/diff");
        assert_eq!(NavItem::Federation.route(), "/federation");
        assert_eq!(NavItem::Speculate.route(), "/speculate");
        assert_eq!(NavItem::WiringImpact.route(), "/wiring/impact");
        assert_eq!(NavItem::Mcp.route(), "/mcp");
        assert_eq!(NavItem::Workspace.route(), "/workspace");
        assert_eq!(NavItem::Wiring.route(), "/wiring");
        assert_eq!(NavItem::Chains.route(), "/chains");
        assert_eq!(NavItem::Orphans.route(), "/orphans");
        assert_eq!(NavItem::Search.route(), "/search");
        assert_eq!(NavItem::Memory.route(), "/memory");
        assert_eq!(NavItem::Plans.route(), "/plans");
        assert_eq!(NavItem::Sessions.route(), "/sessions");
        assert_eq!(NavItem::Cognitive.route(), "/cognitive");
        assert_eq!(NavItem::Health.route(), "/health");
        assert_eq!(NavItem::HooksLive.route(), "/hooks");
        assert_eq!(NavItem::Inspector.route(), "/inspector");
        assert_eq!(NavItem::Settings.route(), "/settings");
    }

    #[test]
    fn nav_iter_yields_all_twenty_one_items() {
        let count = NavItem::iter().count();
        assert_eq!(
            count, 21,
            "sidebar must expose 21 entries (6 Sentrux + 7 CI + 4 Orch + 3 Diag + 1 Sys)"
        );
    }

    #[test]
    fn nav_sections_follow_display_order() {
        let sections: Vec<_> = NavItem::iter().map(NavItem::section).collect();
        for s in &sections[..6] {
            assert_eq!(*s, NavSection::Sentrux);
        }
        for s in &sections[6..13] {
            assert_eq!(*s, NavSection::CodeIntelligence);
        }
        for s in &sections[13..17] {
            assert_eq!(*s, NavSection::Orchestration);
        }
        for s in &sections[17..20] {
            assert_eq!(*s, NavSection::Diagnostics);
        }
        assert_eq!(sections[20], NavSection::System);
    }

    #[test]
    fn every_nav_item_has_distinct_route() {
        use std::collections::HashSet;
        let routes: HashSet<&str> = NavItem::iter().map(NavItem::route).collect();
        assert_eq!(routes.len(), 21, "no duplicate routes");
    }

    #[test]
    fn every_nav_item_has_label_and_icon() {
        for item in NavItem::iter() {
            assert!(!item.label().is_empty(), "{:?} missing label", item);
            assert!(!item.icon().is_empty(), "{:?} missing icon", item);
        }
    }
}
