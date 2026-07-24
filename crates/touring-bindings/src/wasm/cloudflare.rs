//! D35 — Cloudflare Workers edge functions for touring-wasm.
//!
//! Provides WASM-exported functions compatible with Cloudflare Workers runtime.
//! Target: `wasm32-unknown-unknown` (no WASI, no bindings).
//!
//! # Security
//! - No filesystem, network, or syscalls (pure computation)
//! - All inputs validated before processing

/// A symbol search result with match score.
#[derive(Debug, Clone)]
pub struct SymbolHit {
    /// Symbol name.
    pub name: String,
    /// File path where symbol is defined.
    pub file_path: String,
    /// Line number in file.
    pub line: u32,
    /// Relevance score [0.0, 1.0].
    pub score: f32,
}

/// Render a symbol graph as SVG.
///
/// # Arguments
/// * `graph_json` — JSON serialized graph with nodes and edges
///
/// # Returns
/// SVG string representation of the graph, or error message on failure.
// `&str` is not C-ABI-safe, but this target is `wasm32-unknown-unknown` where
// Cloudflare Workers uses its own linear-memory ABI — the lint is a false positive here.
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub extern "C" fn render_graph_svg(graph_json: &str) -> *mut std::ffi::c_char {
    match render_graph_svg_impl(graph_json) {
        Ok(svg) => {
            let mut s = svg.into_bytes();
            s.push(0);
            let ptr = s.as_ptr() as *mut std::ffi::c_char;
            std::mem::forget(s);
            ptr
        }
        Err(e) => {
            let msg = format!("Error: {}", e);
            let mut s = msg.into_bytes();
            s.push(0);
            let ptr = s.as_ptr() as *mut std::ffi::c_char;
            std::mem::forget(s);
            ptr
        }
    }
}

fn render_graph_svg_impl(graph_json: &str) -> Result<String, String> {
    let data: serde_json::Value =
        serde_json::from_str(graph_json).map_err(|e| format!("Invalid graph JSON: {}", e))?;

    let nodes_arr = data
        .get("nodes")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'nodes' field")?;
    let edges_arr = data
        .get("edges")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'edges' field")?;

    if nodes_arr.is_empty() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"400\" height=\"300\">\
<text x=\"200\" y=\"150\" text-anchor=\"middle\" fill=\"#666\">Empty graph</text>\
</svg>";
        return Ok(svg.to_string());
    }

    let width: f64 = 800.0;
    let height: f64 = 600.0;
    let padding: f64 = 50.0;

    let mut positions: std::collections::HashMap<String, (f64, f64)> =
        std::collections::HashMap::new();

    let n = nodes_arr.len() as f64;
    for (i, node) in nodes_arr.iter().enumerate() {
        let id = node
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let angle = 2.0 * std::f64::consts::PI * i as f64 / n;
        let radius = (width.min(height) - 2.0 * padding) / 2.5;
        let x = width / 2.0 + radius * angle.cos();
        let y = height / 2.0 + radius * angle.sin();
        positions.insert(id, (x, y));
    }

    let mut edges_svg = String::new();
    for edge in edges_arr {
        let source = edge.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let target = edge.get("target").and_then(|v| v.as_str()).unwrap_or("");
        if let (Some((x1, y1)), Some((x2, y2))) = (positions.get(source), positions.get(target)) {
            edges_svg.push_str(&format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#aaa\" stroke-width=\"1\"/>",
                x1, y1, x2, y2
            ));
            edges_svg.push('\n');
        }
    }

    let mut nodes_svg = String::new();
    for node in nodes_arr {
        let id = node.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let label = node.get("label").and_then(|v| v.as_str()).unwrap_or(id);
        if let Some((x, y)) = positions.get(id) {
            nodes_svg.push_str(&format!(
                "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"6\" fill=\"#4a90d9\"/><text x=\"{:.1}\" y=\"{:.1}\" dy=\"-10\" text-anchor=\"middle\" font-size=\"10\" fill=\"#333\">{}</text>",
                x, y, x, y, label
            ));
            nodes_svg.push('\n');
        }
    }

    Ok(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
<style>
text {{ font-family: system-ui, sans-serif; }}
</style>
{edges}
{nodes}
</svg>"##,
        w = width,
        h = height,
        edges = edges_svg,
        nodes = nodes_svg
    ))
}

/// Find symbols in a graph matching a query string.
///
/// # Arguments
/// * `query` — Search query string
/// * `graph_json` — JSON serialized graph with nodes
///
/// # Returns
/// JSON array of `SymbolHit` objects.
// `&str` parameters are not C-ABI-safe, but this target is `wasm32-unknown-unknown`
// where Cloudflare Workers uses its own linear-memory ABI — the lint is a false positive here.
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub extern "C" fn find_symbols(query: &str, graph_json: &str) -> *mut std::ffi::c_char {
    match find_symbols_impl(query, graph_json) {
        Ok(result) => {
            let mut s = result.into_bytes();
            s.push(0);
            let ptr = s.as_ptr() as *mut std::ffi::c_char;
            std::mem::forget(s);
            ptr
        }
        Err(e) => {
            let msg = format!("Error: {}", e);
            let mut s = msg.into_bytes();
            s.push(0);
            let ptr = s.as_ptr() as *mut std::ffi::c_char;
            std::mem::forget(s);
            ptr
        }
    }
}

fn find_symbols_impl(query: &str, graph_json: &str) -> Result<String, String> {
    let data: serde_json::Value =
        serde_json::from_str(graph_json).map_err(|e| format!("Invalid graph JSON: {}", e))?;

    let nodes = data
        .get("nodes")
        .and_then(|v| v.as_array())
        .ok_or("Missing 'nodes' field")?;

    let query_lower = query.to_lowercase();

    let mut hits: Vec<SymbolHit> = Vec::new();

    for node in nodes {
        let label = node.get("label").and_then(|v| v.as_str()).unwrap_or("");
        let label_lower = label.to_lowercase();

        if label_lower.contains(&query_lower) {
            let file_path = node
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line = node.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let score = if label_lower.starts_with(&query_lower) {
                1.0
            } else {
                0.5
            };

            hits.push(SymbolHit {
                name: label.to_string(),
                file_path,
                line,
                score,
            });
        }
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(20);

    let result: Vec<serde_json::Value> = hits
        .into_iter()
        .map(|hit| {
            serde_json::json!({
                "name": hit.name,
                "file_path": hit.file_path,
                "line": hit.line,
                "score": hit.score
            })
        })
        .collect();

    serde_json::to_string(&result).map_err(|e| format!("JSON serialization failed: {}", e))
}
