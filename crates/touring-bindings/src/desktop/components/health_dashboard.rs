//! Health dashboard component — displays touring daemon health via `touring doctor -j`.

use crate::desktop::{Theme, spawn_touring_command};
use egui::{Color32, RichText, Ui};
use serde::Deserialize;

/// Component status entry from `touring doctor -j`.
#[derive(Debug, Deserialize)]
pub struct DoctorComponent {
    /// Human-readable name of the component.
    pub name: String,
    /// Status string — `"ok"` means healthy.
    pub status: String,
    /// Additional detail (version string, socket path, etc.).
    pub detail: String,
}

/// Parsed `touring doctor -j` output.
#[derive(Debug, Deserialize)]
pub struct DoctorOutput(pub Vec<DoctorComponent>);

impl DoctorOutput {
    /// Returns `true` if every component status is `"ok"`.
    #[must_use]
    fn all_healthy(&self) -> bool {
        self.0.iter().all(|c| c.status == "ok")
    }
}

/// Health dashboard widget.
///
/// Renders the touring daemon health check (`touring doctor -j`) in a themed panel.
pub struct HealthDashboard;

impl HealthDashboard {
    /// Builds the health dashboard UI.
    ///
    /// Calls `touring doctor -j` and renders one row per component with a coloured
    /// status indicator (green = ok, red = error).  Theme colours follow the
    /// application [`Theme`]: dark uses cyan accents on dark panels; light uses
    /// blue on white.
    pub fn ui(ui: &mut Ui, theme: Theme) {
        ui.heading("Touring Daemon Health");

        // Spawn the touring doctor command synchronously on tokio runtime.
        let output = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime for health dashboard")
            .block_on(spawn_touring_command(&["doctor", "-j"]));

        let doctor_output = match output {
            Ok(raw) => match serde_json::from_str::<DoctorOutput>(&raw) {
                Ok(d) => d,
                Err(e) => {
                    ui.label(format!("[parse error] {}", e));
                    return;
                }
            },
            Err(e) => {
                ui.label(format!("[command error] {}", e));
                return;
            }
        };

        let overall_ok = doctor_output.all_healthy();
        let (_bg_fill, accent) = match theme {
            Theme::Dark => (
                Color32::from_rgb(0x13, 0x16, 0x1f),
                Color32::from_rgb(0x00, 0xd4, 0xff),
            ),
            Theme::Light => (
                Color32::from_rgb(0xf0, 0xf0, 0xf0),
                Color32::from_rgb(0x00, 0x66, 0xcc),
            ),
        };

        // Overall status pill.
        ui.horizontal(|row| {
            let (pill_bg, pill_fg) = if overall_ok {
                (Color32::from_rgb(0x00, 0x80, 0x40), Color32::WHITE)
            } else {
                (Color32::from_rgb(0xcc, 0x00, 0x00), Color32::WHITE)
            };
            row.label(
                RichText::new(if overall_ok { "HEALTHY" } else { "DEGRADED" })
                    .color(pill_fg)
                    .background_color(pill_bg),
            );
        });

        ui.add_space(8.0);

        for comp in &doctor_output.0 {
            let status_color = if comp.status == "ok" {
                Color32::from_rgb(0x00, 0xb0, 0x50)
            } else {
                Color32::from_rgb(0xff, 0x44, 0x44)
            };

            ui.horizontal(|row| {
                let indicator = if comp.status == "ok" { "●" } else { "○" };
                row.label(RichText::new(indicator).color(status_color));
                row.label(RichText::new(&comp.name).color(accent));
                row.label(RichText::new(&comp.detail).small());
            });
            ui.add_space(4.0);
        }
    }
}
