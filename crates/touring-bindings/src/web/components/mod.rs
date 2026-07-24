//! Component library for touring-web.

pub mod error_boundary;
pub mod page_chrome;
pub mod sidebar;
pub mod tables;
pub mod theme_toggle;

// Wave 4 (2026-06-12) — shared page chrome for cross-page harmonization.
pub use page_chrome::{KpiCell, KpiStrip, PageHero, Panel};

// Elite W1 (SPEC 2026-06-12) — global shell, palette, icons, primitives.
pub mod elite_shell;
pub use elite_shell::{EliteShell, PaletteCtx, breadcrumb_for};
pub mod command_palette;
pub use command_palette::{CommandPalette, filter_nav};
pub mod icons;
pub use icons::{Icon, icon_markup};
pub mod area_chart;
pub use area_chart::{AreaChart, AreaSeries};
pub mod progress_track;
pub use progress_track::ProgressTrack;
// Elite W2 (SPEC §4.3) — remaining shared diagram primitives.
pub mod event_ribbon;
pub use event_ribbon::{EventRibbon, RibbonEvent, cap_events};
pub mod pipeline_stages;
pub use pipeline_stages::{PipelineStages, Stage, StageState};
pub mod mini_bars;
pub use mini_bars::{MiniBars, normalize_bars};
// Elite W3 (SPEC §4.3) — BFS rings + isometric palace.
pub mod depth_rings;
pub use depth_rings::{DepthRing, DepthRings, RingNode, node_state, ring_position, state_color};
pub mod iso_palace;
pub use iso_palace::{IsoPalace, Wing, height_fractions, iso_block};

pub use error_boundary::ErrorBoundary;
pub use sidebar::Sidebar;
pub use tables::{DataTable, ScoreBar, StatusDot, escape_html};
pub use theme_toggle::ThemeToggle;
// Wave 4 P10.3 — reusable Sentrux dashboard components.
pub mod sparkline;
pub use sparkline::Sparkline;
pub mod radar_chart;
pub use radar_chart::RadarChart;
pub mod signal_card;
pub use signal_card::SignalCard;
