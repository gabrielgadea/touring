//! App shell — root component wiring contexts, theme and routes into
//! the global [`EliteShell`] (SPEC 2026-06-12 §4.1).
//!
//! Elite W1: pages render inside the shell's `el-main`; the shell owns
//! titlebar, breadcrumb, sidebar, ⌘K palette and the daemon probe.

use crate::web::components::EliteShell;
use crate::web::ctx::{RefreshBus, WorkspaceCtx};
use crate::web::routes::dashboard::Dashboard;
use crate::web::routes::federation::Federation;
use crate::web::routes::health::HealthDashboard;
use crate::web::routes::memory::MemoryRecall;
use crate::web::routes::orphans::OrphanList;
use crate::web::routes::quality::QualityDetail;
use crate::web::routes::quality_diff::QualityDiff;
use crate::web::routes::quality_rules::QualityRules;
use crate::web::routes::search::SymbolSearch;
use crate::web::routes::wiring::WiringGraph;
use crate::web::routes::workspace::WorkspaceGraph;
use crate::web::theme::{apply_theme, theme_signal};
use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
// Wave 4 (2026-06-12) — zip-artboard pages.
use crate::web::routes::chains::WiringChains;
use crate::web::routes::cognitive::CognitiveEngines;
use crate::web::routes::hooks::HooksLive;
use crate::web::routes::plans::PlansBoard;
use crate::web::routes::sessions::SessionsList;
use crate::web::routes::settings::SettingsPage;
// Elite W4 (SPEC §6) — new surfaces.
use crate::web::routes::mcp::McpTools;
use crate::web::routes::speculate::SpeculatePage;
use crate::web::routes::wiring_impact::WiringImpact;
// W6 (SPEC §6.5) — tri-pane inspector.
use crate::web::routes::inspector::InspectorPage;

/// Default page-refresh cadence (seconds) for the [`RefreshBus`].
pub const DEFAULT_REFRESH_SECS: u32 = 30;

/// Top-level Leptos app component — installs meta context, theme,
/// WorkspaceCtx + RefreshBus, the EliteShell and every route.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let theme = theme_signal();
    provide_context(theme);

    // Elite W1 global contexts (SPEC §4.4): workspace + refresh tick.
    WorkspaceCtx::provide();
    let bus = RefreshBus::provide(DEFAULT_REFRESH_SECS);
    bus.start();

    // Apply theme on signal change — theme_signal() already injected CSS at startup.
    Effect::new(move |_| {
        let t = theme.get();
        apply_theme(t);
    });

    view! {
        <Router>
            <div data-theme={move || theme.get().name()}>
                <EliteShell>
                    <Routes fallback=|| view! { <div class="el-empty">"Not Found"</div> }>
                        <Route path=path!("/") view=|| view! { <Dashboard /> } />
                        <Route path=path!("/dashboard") view=|| view! { <Dashboard /> } />
                        <Route path=path!("/quality") view=|| view! { <QualityDetail /> } />
                        <Route path=path!("/quality/rules") view=|| view! { <QualityRules /> } />
                        <Route path=path!("/quality/diff") view=|| view! { <QualityDiff /> } />
                        <Route path=path!("/federation") view=|| view! { <Federation /> } />
                        <Route path=path!("/health") view=|| view! { <HealthDashboard /> } />
                        <Route path=path!("/orphans") view=|| view! { <OrphanList /> } />
                        <Route path=path!("/wiring") view=|| view! { <WiringGraph /> } />
                        <Route path=path!("/workspace") view=|| view! { <WorkspaceGraph /> } />
                        <Route path=path!("/search") view=|| view! { <SymbolSearch /> } />
                        <Route path=path!("/memory") view=|| view! { <MemoryRecall /> } />
                        <Route path=path!("/chains") view=|| view! { <WiringChains /> } />
                        <Route path=path!("/hooks") view=|| view! { <HooksLive /> } />
                        <Route path=path!("/plans") view=|| view! { <PlansBoard /> } />
                        <Route path=path!("/sessions") view=|| view! { <SessionsList /> } />
                        <Route path=path!("/cognitive") view=|| view! { <CognitiveEngines /> } />
                        <Route path=path!("/settings") view=|| view! { <SettingsPage /> } />
                        <Route path=path!("/mcp") view=|| view! { <McpTools /> } />
                        <Route path=path!("/wiring/impact") view=|| view! { <WiringImpact /> } />
                        <Route path=path!("/speculate") view=|| view! { <SpeculatePage /> } />
                        <Route path=path!("/inspector") view=|| view! { <InspectorPage /> } />
                    </Routes>
                </EliteShell>
            </div>
        </Router>
    }
}
