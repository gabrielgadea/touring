//! Application root — `eframe::App` implementation for the touring desktop UI.
//!
//! Organises the UI into a fixed left navigation panel (250 px) and a
//! resizing main content area that switches between the three sub-panels:
//!  - [`HealthDashboard`](components::HealthDashboard)
//!  - [`OrphanListPanel`](components::OrphanListPanel)
//!  - [`WiringGraphViewer`](components::WiringGraphViewer)
//!
//! Theme is toggled via a small button in the top-left corner and persists
//! in [`AppState`].

use crate::desktop::cli;
use crate::desktop::{
    Theme,
    components::{HealthDashboard, OrphanListPanel, WiringGraphViewer},
};
use std::time::Duration;

/// Navigation items available in the left panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavItem {
    /// Touring daemon health dashboard.
    Health,
    /// Orphan symbol list.
    Orphans,
    /// Wiring graph viewer.
    Wiring,
}

impl NavItem {
    fn label(self) -> &'static str {
        match self {
            Self::Health => "Health",
            Self::Orphans => "Orphans",
            Self::Wiring => "Wiring",
        }
    }
}

/// Application state carried through the eframe session.
#[derive(Debug)]
pub struct AppState {
    /// Current theme variant.
    pub theme: Theme,
    /// Currently selected navigation item.
    pub nav: NavItem,
    /// Tokio runtime handle for spawning async refresh tasks.
    _runtime: tokio::runtime::Handle,
}

impl AppState {
    /// Builds a new [`AppState`].
    #[must_use]
    pub fn new(runtime: tokio::runtime::Handle) -> Self {
        Self {
            theme: Theme::Dark,
            nav: NavItem::Health,
            _runtime: runtime,
        }
    }

    /// Returns a reference to the current [`Theme`].
    #[must_use]
    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// Toggles the theme between [`Theme::Dark`] and [`Theme::Light`].
    pub fn toggle_theme(&mut self) {
        self.theme = match self.theme {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        };
    }
}

impl eframe::App for AppState {
    /// Called on every frame — renders the full UI.
    ///
    /// Receives a [`egui::Ui`] already scoped to the root area by eframe.
    /// All sub-panels are rendered with `show_inside(ui, …)` so they nest
    /// correctly inside the provided [`egui::Ui`] rather than painting
    /// directly on the [`egui::Context`].
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let theme = self.theme;
        let ctx = ui.ctx().clone();

        // ── Top bar with theme toggle ─────────────────────────────────────────
        egui::Panel::top("top_bar").show_inside(ui, |ui| {
            ui.horizontal(|row| {
                // Theme toggle button (small, top-left).
                let btn_label = match theme {
                    Theme::Dark => "☀ Light",
                    Theme::Light => "🌙 Dark",
                };
                if row.button(btn_label).clicked() {
                    self.toggle_theme();
                }

                // Spacer — push the rest to the right.
                row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |_| {});
            });
        });

        // ── Left navigation panel (250 px fixed) ───────────────────────────────
        egui::Panel::left("nav_panel")
            .resizable(false)
            .default_size(250.0)
            .show_inside(ui, |ui| {
                // Panel background already themed by egui.
                let _ = ui.heading("Touring");

                ui.add_space(8.0);

                for item in [NavItem::Health, NavItem::Orphans, NavItem::Wiring] {
                    let is_selected = self.nav == item;
                    let label = if is_selected {
                        egui::RichText::new(item.label()).strong()
                    } else {
                        egui::RichText::new(item.label())
                    };
                    let response = ui.selectable_label(is_selected, label);
                    if response.clicked() {
                        self.nav = item;
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |bottom| {
                    bottom.add_space(16.0);
                    if bottom.button("Refresh data").clicked() {
                        let ctx = ctx.clone();
                        tokio::spawn(async move {
                            // Fire-and-forget refresh — just re-spawn the CLI commands.
                            let _ = cli::health_summary().await;
                            let _ = cli::orphans_list().await;
                            let _ = cli::wiring_dot_export().await;
                            ctx.request_repaint();
                        });
                    }
                });
            });

        // ── Main content area ─────────────────────────────────────────────────
        egui::CentralPanel::default().show_inside(ui, |ui| {
            // Schedule a repaint after 30 seconds so data auto-refreshes.
            ctx.request_repaint_after(Duration::from_secs(30));

            match self.nav {
                NavItem::Health => HealthDashboard::ui(ui, theme),
                NavItem::Orphans => OrphanListPanel::ui(ui, theme),
                NavItem::Wiring => WiringGraphViewer::ui(ui, theme),
            }
        });
    }
}

impl AppState {
    /// Constructs the eframe app box — called from `main()`.
    pub fn build_app(_cc: &eframe::CreationContext<'_>) -> Box<dyn eframe::App> {
        // Default to dark theme; egui theme persistence handled by eframe.
        let theme = Theme::Dark;

        let runtime = tokio::runtime::Handle::current();
        let mut state = Self::new(runtime);
        state.theme = theme;

        Box::new(state)
    }
}
