//! Orphan list panel — displays `touring wiring orphans -j` in a themed list.
use crate::desktop::{Theme, spawn_touring_command};
use serde::Deserialize;
/// Single orphan entry from `touring wiring orphans -j`.
#[derive(Debug, Deserialize)]
pub struct OrphanEntry {
    /// File path where the orphan symbol is defined.
    pub module_file: String,
    /// Kind of symbol (struct, function, method, const, enum, trait, etc.).
    pub symbol_kind: String,
    /// Name of the orphan symbol.
    pub symbol_name: String,
    /// Visibility of the symbol (always "public" for orphans).
    #[serde(default)]
    pub visibility: String,
}
/// Parsed `touring wiring orphans -j` output.
#[derive(Debug, Deserialize)]
pub struct WiringOrphansOutput {
    /// Total number of orphan symbols found.
    pub orphan_count: usize,
    /// Detailed list of orphan entries.
    pub orphans: Vec<OrphanEntry>,
    /// Patterns detected as dead code (not currently used).
    #[serde(default)]
    pub dead_patterns: Vec<String>,
}
/// Orphan list panel widget.
///
/// Renders the touring wiring orphans list (`touring wiring orphans -j`) in a
/// themed scrollable list. Each row shows the module file, symbol name, and
/// symbol kind. The orphan count is displayed prominently with theme-aware colours.
pub struct OrphanListPanel;
impl OrphanListPanel {
    /// Builds the orphan list panel UI.
    ///
    /// Calls `touring wiring orphans -j` and renders a scrollable list of
    /// orphan symbols with theme-aware colouring.
    pub fn ui(ui: &mut egui::Ui, theme: Theme) {
        let _ = ui.heading("Orphan Symbols");
        ui.add_space(4.0);
        let output = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime for orphan list panel")
            .block_on(spawn_touring_command(&["wiring", "orphans", "-j"]));
        let orphans_output = match output {
            Ok(raw) => match serde_json::from_str::<WiringOrphansOutput>(&raw) {
                Ok(o) => o,
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
        let (_bg_fill, text_primary, count_color) = match theme {
            Theme::Dark => (
                egui::Color32::from_rgb(0x0c, 0x0e, 0x14),
                egui::Color32::from_rgb(0xe0, 0xe0, 0xe0),
                egui::Color32::from_rgb(0xff, 0xb0, 0x00),
            ),
            Theme::Light => (
                egui::Color32::WHITE,
                egui::Color32::from_rgb(0x1a, 0x1a, 0x1a),
                egui::Color32::from_rgb(0xcc, 0x88, 0x00),
            ),
        };
        ui.horizontal(|row| {
            row.label(egui::RichText::new("Orphan count:").color(text_primary));
            row.label(
                egui::RichText::new(format!("{}", orphans_output.orphan_count))
                    .color(count_color)
                    .strong(),
            );
        });
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |scroll_ui| {
            for orphan in &orphans_output.orphans {
                let kind_label = format!("[{}]", orphan.symbol_kind);
                let kind_color = match orphan.symbol_kind.as_str() {
                    "struct" | "enum" | "trait" => egui::Color32::from_rgb(0x00, 0xcc, 0xff),
                    "function" | "method" => egui::Color32::from_rgb(0x88, 0xff, 0x88),
                    "const" | "static" => egui::Color32::from_rgb(0xff, 0xcc, 0x44),
                    _ => text_primary,
                };
                scroll_ui.horizontal(|row| {
                    let module_truncated = if orphan.module_file.len() > 50 {
                        format!(
                            "...{}",
                            &orphan.module_file[orphan.module_file.len() - 47..]
                        )
                    } else {
                        orphan.module_file.clone()
                    };
                    row.label(
                        egui::RichText::new(module_truncated)
                            .small()
                            .color(egui::Color32::from_rgb(0x99, 0x99, 0x99)),
                    );
                });
                scroll_ui.horizontal(|row| {
                    row.add(egui::Label::new(
                        egui::RichText::new(&orphan.symbol_name)
                            .color(text_primary)
                            .monospace(),
                    ));
                    row.label(egui::RichText::new(kind_label).color(kind_color).small());
                });
                scroll_ui.add_space(6.0);
            }
        });
    }
}
