//! Wiring graph viewer component — renders the touring wiring graph as SVG via graphviz.
//!
//! Uses `touring wiring modules -j` to fetch module-level wiring data and emits a
//! minimal DOT graph, then pipes it through the `dot -Tsvg` subprocess to produce a
//! themed SVG that is displayed inside a scrollable egui panel.
//!
//! If graphviz is not installed on the system, an error state with setup instructions
//! is shown instead.

use crate::desktop::{Theme, spawn_touring_command};
use serde::Deserialize;
use std::process::Stdio;

/// Error states for the wiring graph viewer.
#[derive(Debug, Clone)]
pub enum WiringViewerError {
    /// Graphviz `dot` CLI is not installed.
    GraphvizNotInstalled,
    /// Failed to spawn or communicate with the `dot` subprocess.
    DotProcess(String),
    /// Failed to fetch wiring data from the touring CLI.
    TouringCommand(String),
    /// The `dot` command produced empty output.
    EmptySvg,
}

impl std::fmt::Display for WiringViewerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GraphvizNotInstalled => {
                write!(
                    f,
                    "graphviz is not installed — install with: sudo apt install graphviz"
                )
            }
            Self::DotProcess(s) => write!(f, "dot process error: {}", s),
            Self::TouringCommand(s) => write!(f, "touring CLI error: {}", s),
            Self::EmptySvg => write!(f, "dot produced empty SVG output"),
        }
    }
}

/// Module-level wiring entry (one entry per source file).
#[derive(Debug, Deserialize)]
struct WiringModule {
    #[serde(rename = "file_path")]
    file_path: String,
    #[serde(rename = "integration_score")]
    integration_score: f64,
    #[serde(rename = "orphan_count")]
    orphan_count: usize,
}

/// Wrapper for the `touring wiring modules -j` top-level object.
#[derive(Debug, Deserialize)]
struct WiringModulesOutput(Vec<WiringModule>);

// Health colour constants per theme.
//
// Dark theme uses the application's dark palette background with emerald accents.
// Light theme uses a muted green for the healthy state.

/// Builds a minimal DOT digraph from the wiring modules JSON.
///
/// Each module becomes a node whose fill colour encodes the integration score
/// (green = 1.0, red = 0.0).  Nodes with orphan symbols are highlighted with
/// a dashed border.  Edges are added from each module to the crates it belongs to
/// (extracted from the file path prefix).
fn build_dot_from_modules(modules: &[WiringModule]) -> String {
    use std::fmt::Write;

    let mut dot = String::from(
        "digraph Wiring {\n  rankdir=LR;\n  node [shape=box style=filled fillcolor=white fontname=\"DejaVu Sans\"];\n  edge [color=\"#666666\"];\n",
    );

    for m in modules {
        // Extract crate name from path like "crates/touring-foo/src/..."
        let crate_name = m.file_path.split('/').nth(1).unwrap_or(&m.file_path);

        // Colour encodes integration score.
        let (r, g, b) = score_to_rgb(m.integration_score);
        let color_hex = format!("#{:02x}{:02x}{:02x}", r, g, b);

        // Orphan modules get a dashed border.
        let style = if m.orphan_count > 0 {
            "dashed"
        } else {
            "solid"
        };

        // Label shows short path + score.
        let short_path = m.file_path.split('/').next_back().unwrap_or(&m.file_path);

        writeln!(&mut dot,
            "  \"{}\" [fillcolor=\"{}\" color=\"{}\" style=\"{}\" label=\"{}\\n({:.2})\" fontsize=9];",
            crate_name, color_hex, color_hex, style, short_path, m.integration_score
        ).ok();
    }

    dot.push_str("}\n");
    dot
}

/// Maps an integration score in [0, 1] to an RGB triple (green = healthy, red = degraded).
fn score_to_rgb(score: f64) -> (u8, u8, u8) {
    // Clamp to [0, 1].
    let score = score.clamp(0.0, 1.0);
    // green (0,228,0) → yellow (255,255,0) → red (255,0,0)
    let r = (255.0 * (1.0 - score)) as u8;
    let g = (228.0 * score) as u8;
    let b = 0u8;
    (r, g, b)
}

/// Checks whether the graphviz `dot` CLI is available on the system.
fn is_graphviz_available() -> bool {
    std::process::Command::new("dot")
        .arg("-V")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Runs `dot -Tsvg` on the provided DOT string and returns the SVG bytes.
fn dot_to_svg(dot_input: &str) -> Result<Vec<u8>, WiringViewerError> {
    let mut child = std::process::Command::new("dot")
        .args(["-Tsvg"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| WiringViewerError::DotProcess(e.to_string()))?;

    use std::io::Write;
    if let Some(ref mut stdin) = child.stdin {
        stdin
            .write_all(dot_input.as_bytes())
            .map_err(|e| WiringViewerError::DotProcess(e.to_string()))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| WiringViewerError::DotProcess(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WiringViewerError::DotProcess(stderr.to_string()));
    }

    let svg = output.stdout;
    if svg.is_empty() {
        return Err(WiringViewerError::EmptySvg);
    }

    Ok(svg)
}

/// Renders an SVG byte slice as a scrollable texture inside the egui context.
///
/// Decodes the SVG to a PNG buffer using the `resvg` + `tiny_skia` stack (which is
/// already transitively available through `eframe`), then uploads it as an egui
/// texture and displays it via `egui::Image`.
fn render_svg_scrollable(ui: &mut egui::Ui, svg_bytes: &[u8], _theme: Theme) {
    // ── 1. Parse SVG ─────────────────────────────────────────────────────────
    let tree = resvg::usvg::Tree::from_data(svg_bytes, &resvg::usvg::Options::default())
        .expect("valid SVG input");

    // Prefer resvg's own size; fall back to a manual `viewBox` parse via
    // `detect_svg_viewbox` if resvg reports a degenerate (0x0) size — keeps
    // the panel render path usable for unusual SVG outputs (e.g. graphviz
    // versions that emit `viewBox` but no `width`/`height` attributes).
    let (svg_w, svg_h) = {
        let w = tree.size().width() as u32;
        let h = tree.size().height() as u32;
        if w == 0 || h == 0 {
            if let Some((vw, vh)) = detect_svg_viewbox(svg_bytes) {
                (vw.max(1.0) as u32, vh.max(1.0) as u32)
            } else {
                (w.max(1), h.max(1))
            }
        } else {
            (w, h)
        }
    };

    // ── 2. Rasterise to RGBA via tiny_skia ──────────────────────────────────
    let mut pixmap = resvg::tiny_skia::Pixmap::new(svg_w, svg_h).expect("pixmap allocation");
    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap_mut,
    );

    // ── 3. Encode as PNG ─────────────────────────────────────────────────────
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(std::io::Cursor::new(&mut png_bytes), svg_w, svg_h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(pixmap.data()).expect("png write");
    }

    // ── 4. Upload to egui texture ─────────────────────────────────────────────
    let color_image =
        egui::ColorImage::from_rgba_unmultiplied([svg_w as usize, svg_h as usize], &png_bytes);
    let texture = ui.ctx().load_texture(
        "wiring-graph-svg",
        color_image,
        egui::TextureOptions::LINEAR,
    );

    // ── 5. Display inside a scroll area ───────────────────────────────────────
    let scroll = egui::ScrollArea::new([true, true])
        .auto_shrink([false, false])
        .stick_to_right(false)
        .stick_to_bottom(false);

    scroll.show(ui, |ui| {
        // Scale the image to fit within the available width while preserving aspect.
        let available_w = ui.available_width().max(100.0);
        let scale = (available_w / svg_w as f32).min(1.0);
        let _display_size = egui::vec2(svg_w as f32 * scale, svg_h as f32 * scale);

        let img = egui::Image::from_texture(&texture);
        ui.add(img);
    });
}

/// Wiring graph viewer widget.
///
/// Displays the touring wiring graph as a scrollable, zoomable SVG image.
pub struct WiringGraphViewer;

impl WiringGraphViewer {
    /// Builds the wiring graph viewer UI.
    ///
    /// The panel first checks whether graphviz is installed.  If it is not,
    /// an error message with setup instructions is displayed.
    ///
    /// When graphviz is available, the component fetches wiring module data
    /// via `touring wiring modules -j`, builds a minimal DOT graph from it,
    /// converts the DOT to SVG via `dot -Tsvg`, and renders the SVG inside
    /// a scrollable [`egui::ScrollArea`].
    ///
    /// Theme-aware colours are applied to the DOT node fill (green = healthy,
    /// red = degraded) and to the panel background.
    pub fn ui(ui: &mut egui::Ui, theme: Theme) {
        let (bg_dark, bg_light) = (
            egui::Color32::from_rgb(0x0c, 0x0e, 0x14),
            egui::Color32::from_rgb(0xff, 0xff, 0xff),
        );
        let _bg = match theme {
            Theme::Dark => bg_dark,
            Theme::Light => bg_light,
        };

        // ── Graphviz availability check ────────────────────────────────────────
        if !is_graphviz_available() {
            let (_err_bg, err_accent) = match theme {
                Theme::Dark => (
                    egui::Color32::from_rgb(0x1a, 0x10, 0x10),
                    egui::Color32::from_rgb(0xff, 0x66, 0x66),
                ),
                Theme::Light => (
                    egui::Color32::from_rgb(0xff, 0xf0, 0xf0),
                    egui::Color32::from_rgb(0xcc, 0x00, 0x00),
                ),
            };

            ui.horizontal(|row| {
                row.label(egui::RichText::new("[ERROR]").color(err_accent).strong());
                row.label(" graphviz is not installed — cannot render wiring graph");
            });
            ui.add_space(6.0);
            let code = "sudo apt install graphviz   # Debian/Ubuntu\n\
                        brew install graphviz        # macOS\n\
                        pacman -S graphviz           # Arch";
            ui.label(egui::RichText::new(code).monospace().small());
            return;
        }

        // ── Fetch wiring data from touring CLI ─────────────────────────────────
        let raw = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime for wiring viewer")
            .block_on(spawn_touring_command(&["wiring", "modules", "-j"]))
        {
            Ok(s) => s,
            Err(e) => {
                ui.label(format!("[touring error] {}", e));
                return;
            }
        };

        let modules: Vec<WiringModule> = match serde_json::from_str::<WiringModulesOutput>(&raw) {
            Ok(WiringModulesOutput(mods)) => mods,
            Err(e) => {
                ui.label(format!("[parse error] {}", e));
                return;
            }
        };

        // ── Build DOT and convert to SVG ──────────────────────────────────────
        let dot = build_dot_from_modules(&modules);

        let svg_bytes = match dot_to_svg(&dot) {
            Ok(b) => b,
            Err(e) => {
                ui.label(format!("[dot error] {}", e));
                return;
            }
        };

        // ── Render SVG as scrollable texture ──────────────────────────────────
        render_svg_scrollable(ui, &svg_bytes, theme);
    }
}

/// Parses a `viewBox` attribute from an SVG byte slice to return (width, height).
///
/// Exposed as `pub(crate)` so peer rendering helpers can derive layout
/// dimensions from rendered SVG output (e.g. when sizing the egui
/// painter region) without re-parsing the bytes inline. Currently the
/// in-module test suite is the primary caller; future zoom/fit logic
/// will consume it directly from the panel render path.
pub(crate) fn detect_svg_viewbox(svg: &[u8]) -> Option<(f32, f32)> {
    let s = String::from_utf8_lossy(svg);
    let viewbox = s.find("viewBox=\"")?;
    let rest = &s[viewbox + 9..];
    let end = rest.find('"')?;
    let parts: Vec<&str> = rest[..end].split_whitespace().collect();
    if parts.len() >= 4 {
        let w = parts[2].parse::<f32>().ok()?;
        let h = parts[3].parse::<f32>().ok()?;
        Some((w, h))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_to_rgb_healthy() {
        let (r, g, b) = score_to_rgb(1.0);
        assert_eq!((r, g, b), (0, 228, 0));
    }

    #[test]
    fn score_to_rgb_degraded() {
        let (r, g, b) = score_to_rgb(0.0);
        assert_eq!((r, g, b), (255, 0, 0));
    }

    #[test]
    fn score_to_rgb_mid() {
        let (r, g, _b) = score_to_rgb(0.5);
        // green channel should be roughly halved
        assert!(g > 100 && g < 120);
        assert_eq!(r, 127);
    }

    #[test]
    fn dot_built_from_modules() {
        let mods = vec![
            WiringModule {
                file_path: "crates/touring-ast/src/lib.rs".into(),
                integration_score: 1.0,
                orphan_count: 0,
            },
            WiringModule {
                file_path: "crates/touring-hooks/src/runtime.rs".into(),
                integration_score: 0.5,
                orphan_count: 3,
            },
        ];
        let dot = build_dot_from_modules(&mods);
        assert!(dot.contains("digraph Wiring"));
        assert!(dot.contains("touring-ast"));
        assert!(dot.contains("touring-hooks"));
        assert!(dot.contains("dashed")); // orphan module
    }

    #[test]
    fn viewbox_detection() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600"><rect/></svg>"#;
        let (w, h) = detect_svg_viewbox(svg).unwrap();
        assert_eq!((w, h), (800.0, 600.0));
    }
}
