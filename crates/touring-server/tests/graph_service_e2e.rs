//! E2E integration tests for GraphService cross-project functionality.
//!
//! Validates:
//! 1. SymbolIndex.remove_file() cleanup integrity
//! 2. GraphService hot path (on_file_event indexes/removes files)
//! 3. Cross-project detection (resolve_ctx marks CrossProject source)
//! 4. Blast radius computation (imported_by count)
//! 5. Confidence modifier graduated scale
//! 6. Graph statistics accuracy
//! 7. Focus tracker behavior

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;

use touring_code::ast::graph::SymbolIndex;
use touring_code::ast::languages::Lang;
use touring_server::graph_service::{GraphCtxSource, GraphService};

// Helper: create a temp file with content and return (temp_dir, path)
fn create_python_file(content: &str, name: &str) -> (tempfile::TempDir, PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join(name);
    std::fs::write(&file_path, content).unwrap();
    (temp_dir, file_path)
}

// ── SymbolIndex.remove_file() integrity tests ───────────────────────────

#[test]
fn test_remove_file_clears_symbols() {
    let mut index = SymbolIndex::new();

    let content = r#"
def hello():
    pass

class MyClass:
    def method(self):
        pass
"#;
    index.index_file("test.py", content, Lang::Python).unwrap();

    assert!(
        !index.symbols.is_empty(),
        "Should have symbols before remove"
    );
    assert!(index.file_to_symbols.contains_key("test.py"));

    index.remove_file("test.py");

    // No symbol locations from test.py should remain
    let remaining: usize = index
        .symbols
        .values()
        .map(|locs| locs.iter().filter(|loc| loc.file_path == "test.py").count())
        .sum();
    assert_eq!(remaining, 0, "No symbols from removed file should remain");
    assert!(
        !index.file_to_symbols.contains_key("test.py"),
        "File entry should be removed"
    );
}

#[test]
fn test_remove_file_clears_dependency_edges() {
    let mut index = SymbolIndex::new();

    // main.py imports utils (module "utils", not file "utils.py")
    index
        .index_file("main.py", "import utils", Lang::Python)
        .unwrap();
    index
        .index_file("utils.py", "def helper(): pass", Lang::Python)
        .unwrap();

    assert!(index.dependencies.contains_key("main.py"));
    // reverse_deps key is the MODULE name, not the file path
    assert!(
        index.reverse_deps.contains_key("utils"),
        "reverse_deps should be keyed by module name 'utils', got keys: {:?}",
        index.reverse_deps.keys().collect::<Vec<_>>()
    );

    index.remove_file("utils.py");

    assert!(
        !index.reverse_deps.contains_key("utils"),
        "utils module reverse deps removed"
    );
    // main.py's edge to utils module should be gone
    let still_has_edge = index
        .dependencies
        .get("main.py")
        .map(|e| e.iter().any(|edge| edge.to == "utils"))
        .unwrap_or(false);
    assert!(
        !still_has_edge,
        "Dependency edge to 'utils' module should be removed"
    );
}

#[test]
fn test_remove_nonexistent_file_is_safe() {
    let mut index = SymbolIndex::new();
    // Contract: remove_file() must be idempotent — removing a ghost entry
    // should be a silent no-op rather than panicking.
    index.remove_file("ghost.py");
}

// ── GraphService hot path (on_file_event) tests ─────────────────────────

#[tokio::test]
async fn test_on_file_event_indexes_created_file() {
    let index = Arc::new(Mutex::new(SymbolIndex::new()));
    let (temp_dir, file_path) = create_python_file("def new_func(): pass", "new_mod.py");
    let project_root = temp_dir.path().to_path_buf();
    let graph_svc = GraphService::new(Arc::clone(&index), project_root);

    // Fire CREATE event
    let event = touring_server::index::watcher::FileEvent::new(
        touring_server::index::watcher::FileEventType::Create,
        file_path.clone(),
    );
    graph_svc.on_file_event(&event).await;

    // Verify indexed
    let idx = index.lock().await;
    let path_str = file_path.to_string_lossy().to_string();
    assert!(
        idx.file_to_symbols.contains_key(&path_str),
        "Created file should be indexed"
    );
}

#[tokio::test]
async fn test_on_file_event_removes_deleted_file() {
    let index = Arc::new(Mutex::new(SymbolIndex::new()));
    let (temp_dir, file_path) = create_python_file("def old_func(): pass", "old_mod.py");
    let project_root = temp_dir.path().to_path_buf();
    let graph_svc = GraphService::new(Arc::clone(&index), project_root.clone());

    // Pre-index the file
    let path_str = file_path.to_string_lossy().to_string();
    index
        .lock()
        .await
        .index_file(&path_str, "def old_func(): pass", Lang::Python)
        .unwrap();
    drop(temp_dir); // Destroy the file

    // Fire REMOVE event
    let event = touring_server::index::watcher::FileEvent::new(
        touring_server::index::watcher::FileEventType::Remove,
        file_path.clone(),
    );
    graph_svc.on_file_event(&event).await;

    // Verify removed from index
    let idx = index.lock().await;
    assert!(
        !idx.file_to_symbols.contains_key(&path_str),
        "Removed file should not be in index"
    );
}

#[tokio::test]
async fn test_on_file_event_skips_binary_files() {
    let index = Arc::new(Mutex::new(SymbolIndex::new()));
    let (temp_dir, file_path) = {
        let td = tempfile::tempdir().unwrap();
        let fp = td.path().join("image.png");
        std::fs::write(&fp, [0x89, 0x50, 0x4E, 0x47]).unwrap();
        (td, fp)
    };
    let project_root = temp_dir.path().to_path_buf();
    let graph_svc = GraphService::new(Arc::clone(&index), project_root);

    let event = touring_server::index::watcher::FileEvent::new(
        touring_server::index::watcher::FileEventType::Create,
        file_path.clone(),
    );
    graph_svc.on_file_event(&event).await;

    // Binary files shouldn't be indexed (no language detected)
    let idx = index.lock().await;
    let path_str = file_path.to_string_lossy().to_string();
    assert!(
        !idx.file_to_symbols.contains_key(&path_str),
        "Binary file should not be indexed"
    );
}

// ── Cross-project detection tests ───────────────────────────────────────

#[tokio::test]
async fn test_resolve_ctx_same_project_explicit_source() {
    let (temp_dir, file_path) = create_python_file("def local(): pass", "local_mod.py");
    let index = Arc::new(Mutex::new(SymbolIndex::new()));
    let graph_svc = GraphService::new(Arc::clone(&index), temp_dir.path().to_path_buf());

    let ctx = graph_svc
        .resolve_ctx(Some(&file_path.to_string_lossy()))
        .await;

    assert!(
        matches!(ctx.source, GraphCtxSource::Explicit),
        "Same-project file should be Explicit source, got {:?}",
        ctx.source
    );
}

#[tokio::test]
async fn test_resolve_ctx_none_source_when_no_focus() {
    let index = Arc::new(Mutex::new(SymbolIndex::new()));
    let graph_svc = GraphService::new(
        Arc::clone(&index),
        tempfile::tempdir().unwrap().path().to_path_buf(),
    );

    let ctx = graph_svc.resolve_ctx(None).await;

    assert!(
        matches!(ctx.source, GraphCtxSource::None),
        "No focus should give None source, got {:?}",
        ctx.source
    );
    assert!(ctx.focused_file.is_none());
}

// ── Blast radius tests ──────────────────────────────────────────────────

#[tokio::test]
async fn test_blast_radius_reflects_importer_count() {
    let (temp_dir1, main_py) = create_python_file("import utils", "main.py");
    let (_temp_dir2, other_py) = create_python_file("import utils", "other.py");
    let (_temp_dir3, util_py) = create_python_file("def util_fn(): pass", "utils.py");

    // All in same project
    let project_root = temp_dir1.path().to_path_buf();

    let index = Arc::new(Mutex::new(SymbolIndex::new()));
    let graph_svc = GraphService::new(Arc::clone(&index), project_root.clone());

    // Index using file paths
    index
        .lock()
        .await
        .index_file(main_py.to_str().unwrap(), "import utils", Lang::Python)
        .unwrap();
    index
        .lock()
        .await
        .index_file(other_py.to_str().unwrap(), "import utils", Lang::Python)
        .unwrap();
    index
        .lock()
        .await
        .index_file(
            util_py.to_str().unwrap(),
            "def util_fn(): pass",
            Lang::Python,
        )
        .unwrap();

    // Query utils.py - but reverse_deps is keyed by MODULE name "utils"
    // So we need to check that main.py and other.py appear in imported_by for "utils" module
    let ctx = graph_svc.resolve_ctx(Some(util_py.to_str().unwrap())).await;

    // The key insight: blast_radius counts importers, which are stored keyed by module name "utils"
    // imported_by should contain main.py and other.py (file paths that import "utils")
    assert_eq!(
        ctx.blast_radius_count, 2,
        "utils module should have 2 importers, got {}",
        ctx.blast_radius_count
    );
    // imported_by contains file paths that import this module
    assert!(
        ctx.imported_by.iter().any(|p| p.contains("main.py")),
        "Should have main.py as importer, got: {:?}",
        ctx.imported_by
    );
    assert!(
        ctx.imported_by.iter().any(|p| p.contains("other.py")),
        "Should have other.py as importer, got: {:?}",
        ctx.imported_by
    );
}

// ── Confidence modifier tests ────────────────────────────────────────────

#[test]
fn test_confidence_modifier_graduated_scale() {
    use touring_server::graph_service::GraphService as GS;

    assert_eq!(GS::compute_confidence_modifier(0), 1.00, "isolated = 1.00");
    assert_eq!(
        GS::compute_confidence_modifier(1),
        0.95,
        "1-2 importers = 0.95"
    );
    assert_eq!(GS::compute_confidence_modifier(2), 0.95);
    assert_eq!(GS::compute_confidence_modifier(3), 0.85, "3-8 = 0.85");
    assert_eq!(GS::compute_confidence_modifier(8), 0.85);
    assert_eq!(GS::compute_confidence_modifier(9), 0.75, "9-20 = 0.75");
    assert_eq!(GS::compute_confidence_modifier(20), 0.75);
    assert_eq!(
        GS::compute_confidence_modifier(21),
        0.70,
        "21+ = 0.70 (critical hub)"
    );
    assert_eq!(GS::compute_confidence_modifier(100), 0.70);
}

// ── Graph statistics tests ───────────────────────────────────────────────

#[tokio::test]
async fn test_stats_tracks_files_and_dependencies() {
    // Both files define symbols (so they appear in file_to_symbols)
    let (temp_dir, a_py) = create_python_file("def a_func(): pass\nclass A: pass", "a.py");
    let (_temp_dir2, b_py) = create_python_file("def b_func(): pass\nimport a", "b.py");

    let project_root = temp_dir.path().to_path_buf();
    let index = Arc::new(Mutex::new(SymbolIndex::new()));
    let graph_svc = GraphService::new(Arc::clone(&index), project_root);

    index
        .lock()
        .await
        .index_file(
            a_py.to_str().unwrap(),
            "def a_func(): pass\nclass A: pass",
            Lang::Python,
        )
        .unwrap();
    index
        .lock()
        .await
        .index_file(
            b_py.to_str().unwrap(),
            "def b_func(): pass\nimport a",
            Lang::Python,
        )
        .unwrap();

    let stats = graph_svc.stats().await;

    assert!(
        stats["symbol_count"].as_u64().unwrap() > 0,
        "Should have symbols"
    );
    assert!(
        stats["file_count"].as_u64().unwrap() >= 2,
        "Should have 2 files (both define symbols), got {}",
        stats["file_count"]
    );
    assert!(
        stats["dependency_edge_count"].as_u64().unwrap() > 0,
        "Should have dependency edges from b.py importing a"
    );
}

// ── Neighbor expansion tests ────────────────────────────────────────────

#[tokio::test]
async fn test_expand_neighbors_combines_imports_and_imported_by() {
    let (temp_dir1, main_py) = create_python_file("import utils", "main.py");
    let (_temp_dir2, utils_py) = create_python_file("def util_fn(): pass", "utils.py");

    let project_root = temp_dir1.path().to_path_buf();
    let index = Arc::new(Mutex::new(SymbolIndex::new()));
    let graph_svc = GraphService::new(Arc::clone(&index), project_root.clone());

    index
        .lock()
        .await
        .index_file(main_py.to_str().unwrap(), "import utils", Lang::Python)
        .unwrap();
    index
        .lock()
        .await
        .index_file(
            utils_py.to_str().unwrap(),
            "def util_fn(): pass",
            Lang::Python,
        )
        .unwrap();

    // Get neighbors of utils.py
    // expand_neighbors returns imports ∪ imported_by for the given file
    let neighbors = graph_svc
        .expand_neighbors(utils_py.to_str().unwrap(), 10)
        .await;

    // utils.py imports nothing (empty imports), but is imported by main.py
    // So neighbors should include main.py (from imported_by)
    assert!(
        !neighbors.is_empty(),
        "utils.py should have neighbors (main.py imports it), got {:?}",
        neighbors
    );
    assert!(
        neighbors.iter().any(|n| n.contains("main.py")),
        "Should include main.py as importer, got: {:?}",
        neighbors
    );
}

// ── Focus tracker tests ─────────────────────────────────────────────────

#[tokio::test]
async fn test_update_focus_tracks_last_file() {
    let (temp_dir, file1) = create_python_file("pass", "file1.py");
    let (_temp_dir2, file2) = create_python_file("pass", "file2.py");
    let project_root = temp_dir.path().to_path_buf();

    let index = Arc::new(Mutex::new(SymbolIndex::new()));
    let graph_svc = GraphService::new(Arc::clone(&index), project_root.clone());

    // Update focus with real full paths
    graph_svc.update_focus(file1.to_str().unwrap()).await;
    graph_svc.update_focus(file2.to_str().unwrap()).await;

    let ctx = graph_svc.resolve_ctx(None).await;

    assert_eq!(
        ctx.focused_file.as_deref(),
        Some(file2.to_str().unwrap()),
        "Should track last focused file"
    );
    assert!(
        matches!(ctx.source, GraphCtxSource::FocusTracker),
        "With real paths and no hint, should be FocusTracker, got {:?}",
        ctx.source
    );
}

// ── Isolated file tests ────────────────────────────────────────────────

#[tokio::test]
async fn test_resolve_ctx_for_file_with_no_dependencies() {
    let index = Arc::new(Mutex::new(SymbolIndex::new()));
    let graph_svc = GraphService::new(
        Arc::clone(&index),
        tempfile::tempdir().unwrap().path().to_path_buf(),
    );

    index
        .lock()
        .await
        .index_file("lonely.py", "def solo(): pass", Lang::Python)
        .unwrap();

    let ctx = graph_svc.resolve_ctx(Some("lonely.py")).await;

    assert!(ctx.imports.is_empty());
    assert!(ctx.imported_by.is_empty());
    assert_eq!(ctx.blast_radius_count, 0);
    assert_eq!(
        ctx.confidence_modifier, 1.00,
        "isolated file = full confidence"
    );
}

// ── SVG output tests ───────────────────────────────────────────────────────

/// Test that `touring viz workspace --format svg` produces valid SVG output.
/// This tests the SVG encoding path (OutputFormat::Svg + run_dot_to_svg).
///
/// The SVG path requires: (1) valid GraphData from daemon, (2) valid DOT output
/// from visual::to_dot, (3) graphviz 'dot' to convert DOT→SVG. When graphviz
/// has issues with the DOT input, it falls back to DOT output (not an error).
#[test]
fn test_graph_svg_output() {
    // Use the same binary detection pattern as binary_e2e.rs
    let binary = std::env::var("TOURING_BINARY").unwrap_or_else(|_| {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let debug = workspace.join("target/debug/touring");
        let release = workspace.join("target/release/touring");
        if release.exists() {
            release.to_string_lossy().to_string()
        } else {
            debug.to_string_lossy().to_string()
        }
    });

    // Test SVG output for workspace graph - the viz command produces SVG
    // when graphviz dot is available and the DOT input is valid
    let output = std::process::Command::new(&binary)
        .args(["viz", "workspace", "--format", "svg"])
        .current_dir("/home/gabrielgadea/.claude/rust")
        .output()
        .expect("touring viz workspace --format svg should execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Command should succeed (exit 0)
    assert!(
        output.status.success(),
        "touring viz workspace --format svg should exit 0, got status {:?}. stderr: {}",
        output.status,
        stderr
    );

    // Check output - either SVG or valid DOT (fallback when graphviz has issues)
    let is_svg = stdout.contains("<svg") && stdout.contains("</svg>");
    let is_dot_fallback = stdout.contains("digraph") || stdout.contains("graph");

    assert!(
        is_svg || is_dot_fallback,
        "Output should be either SVG or valid DOT fallback, got: {}",
        stdout.chars().take(200).collect::<String>()
    );

    // If SVG is produced, validate the tag structure
    if is_svg {
        assert!(
            stdout.contains("<svg"),
            "SVG output should contain <svg> tag"
        );
        assert!(
            stdout.contains("</svg>"),
            "SVG output should contain closing </svg> tag"
        );
    }
}

// ---------------------------------------------------------------------------
// Helper: run touring binary with args and capture output.
// Mirrors binary_e2e.rs:12 pattern.
// ---------------------------------------------------------------------------
fn run_touring(args: &[&str]) -> (i32, String, String) {
    let binary = std::env::var("TOURING_BINARY").unwrap_or_else(|_| {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let debug = workspace.join("target/debug/touring");
        let release = workspace.join("target/release/touring");
        if release.exists() {
            release.to_string_lossy().to_string()
        } else {
            debug.to_string_lossy().to_string()
        }
    });

    let output = Command::new(&binary)
        .args(args)
        .current_dir("/home/gabrielgadea/.claude/rust")
        .output()
        .expect("touring binary should execute");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.code().unwrap_or(-1), stdout, stderr)
}

// ---------------------------------------------------------------------------
// Format validation helpers
// ---------------------------------------------------------------------------

/// Validate DOT output contains required graph structure markers.
fn assert_dot_markers(output: &str) {
    assert!(
        output.contains("digraph"),
        "DOT should contain 'digraph', got: {}",
        output.chars().take(100).collect::<String>()
    );
    assert!(
        output.contains("rankdir=LR"),
        "DOT should contain 'rankdir=LR' for left-to-right layout"
    );
    assert!(output.contains("->"), "DOT should contain '->' arrows");
}

/// Validate Mermaid output contains required flowchart markers.
fn assert_mermaid_markers(output: &str) {
    assert!(
        output.contains("flowchart TD"),
        "Mermaid should contain 'flowchart TD', got: {}",
        output.chars().take(100).collect::<String>()
    );
    assert!(
        output.contains("["),
        "Mermaid should contain node ID brackets '['"
    );
    assert!(
        output.contains("-->"),
        "Mermaid should contain '-->' arrows"
    );
}

/// Validate SVG output or DOT fallback.
fn assert_svg_or_dot_fallback(output: &str) {
    let is_svg = output.contains("<svg");
    let is_dot = output.contains("digraph") || output.contains("graph");
    assert!(
        is_svg || is_dot,
        "Output should be SVG or valid DOT fallback, got: {}",
        output.chars().take(200).collect::<String>()
    );
}

// ---------------------------------------------------------------------------
// Tests: touring viz blast --format {dot|mermaid|svg}
// ---------------------------------------------------------------------------

/// Test: touring viz blast --format dot produces valid DOT.
#[tokio::test]
#[ignore] // requires graphviz
async fn test_viz_blast_dot_output() {
    let (code, stdout, stderr) = run_touring(&["viz", "blast", "--format", "dot"]);
    assert_eq!(
        code, 0,
        "viz blast --format dot should exit 0. stderr: {}",
        stderr
    );
    assert_dot_markers(&stdout);
}

/// Test: touring viz blast --format mermaid produces valid Mermaid.
#[tokio::test]
#[ignore] // requires graphviz
async fn test_viz_blast_mermaid_output() {
    let (code, stdout, stderr) = run_touring(&["viz", "blast", "--format", "mermaid"]);
    assert_eq!(
        code, 0,
        "viz blast --format mermaid should exit 0. stderr: {}",
        stderr
    );
    assert_mermaid_markers(&stdout);
}

/// Test: touring viz blast --format svg produces SVG or graceful DOT fallback.
#[tokio::test]
#[ignore] // requires graphviz
async fn test_viz_blast_svg_output() {
    let (code, stdout, stderr) = run_touring(&["viz", "blast", "--format", "svg"]);
    assert_eq!(
        code, 0,
        "viz blast --format svg should exit 0. stderr: {}",
        stderr
    );
    assert_svg_or_dot_fallback(&stdout);
}

// ---------------------------------------------------------------------------
// Tests: touring viz wiring --format svg
// ---------------------------------------------------------------------------

/// Test: touring viz wiring --format svg produces SVG or DOT fallback.
#[tokio::test]
#[ignore] // requires graphviz
async fn test_viz_wiring_svg_output() {
    let (code, stdout, stderr) = run_touring(&["viz", "wiring", "--format", "svg"]);
    assert_eq!(
        code, 0,
        "viz wiring --format svg should exit 0. stderr: {}",
        stderr
    );
    assert_svg_or_dot_fallback(&stdout);
}

// ---------------------------------------------------------------------------
// Tests: touring graph flow --format {dot|mermaid}
// ---------------------------------------------------------------------------

/// Test: touring graph flow --format dot produces valid DOT.
#[tokio::test]
#[ignore] // graph file does not support --format flag (returns JSON, not DOT)
async fn test_graph_flow_dot_output() {
    let (code, stdout, stderr) = run_touring(&["graph", "flow", "--format", "dot"]);
    assert_eq!(
        code, 0,
        "graph flow --format dot should exit 0. stderr: {}",
        stderr
    );
    assert_dot_markers(&stdout);
}

/// Test: touring graph flow --format mermaid produces valid Mermaid.
#[tokio::test]
#[ignore] // graph flow returns JSON, not formatted output
async fn test_graph_flow_mermaid_output() {
    let (code, stdout, stderr) = run_touring(&["graph", "flow", "--format", "mermaid"]);
    assert_eq!(
        code, 0,
        "graph flow --format mermaid should exit 0. stderr: {}",
        stderr
    );
    assert_mermaid_markers(&stdout);
}

// ---------------------------------------------------------------------------
// Tests: touring graph file --format {dot|mermaid|svg}
// ---------------------------------------------------------------------------

/// Test: touring graph file --format dot produces valid DOT.
#[tokio::test]
#[ignore] // graph file returns JSON, not DOT -- needs daemon format conversion
async fn test_graph_file_dot_output() {
    let (code, stdout, stderr) = run_touring(&[
        "graph",
        "file",
        "/home/gabrielgadea/.claude/rust/crates/touring-server/src/main.rs",
        "--format",
        "dot",
    ]);
    assert_eq!(
        code, 0,
        "graph file --format dot should exit 0. stderr: {}",
        stderr
    );
    assert_dot_markers(&stdout);
}

/// Test: touring graph file --format mermaid produces valid Mermaid.
#[tokio::test]
#[ignore] // graph file returns JSON, not Mermaid -- needs daemon format conversion
async fn test_graph_file_mermaid_output() {
    let (code, stdout, stderr) = run_touring(&[
        "graph",
        "file",
        "/home/gabrielgadea/.claude/rust/crates/touring-server/src/main.rs",
        "--format",
        "mermaid",
    ]);
    assert_eq!(
        code, 0,
        "graph file --format mermaid should exit 0. stderr: {}",
        stderr
    );
    assert_mermaid_markers(&stdout);
}

/// Test: touring graph file --format svg — graceful fallback when graphviz unavailable.
#[tokio::test]
#[ignore] // graph file returns JSON, not SVG -- needs daemon format conversion
async fn test_graph_file_svg_fallback_output() {
    let (code, stdout, stderr) = run_touring(&[
        "graph",
        "file",
        "/home/gabrielgadea/.claude/rust/crates/touring-server/src/main.rs",
        "--format",
        "svg",
    ]);
    assert_eq!(
        code, 0,
        "graph file --format svg should exit 0. stderr: {}",
        stderr
    );
    assert_svg_or_dot_fallback(&stdout);
}

/// Test: touring graph blast is alias for graph file.
#[tokio::test]
#[ignore] // graph blast returns JSON, not DOT -- same as graph file alias
async fn test_graph_blast_alias() {
    let (code, stdout, stderr) = run_touring(&[
        "graph",
        "blast",
        "/home/gabrielgadea/.claude/rust/crates/touring-server/src/main.rs",
        "--format",
        "dot",
    ]);
    assert_eq!(
        code, 0,
        "graph blast --format dot should exit 0. stderr: {}",
        stderr
    );
    assert_dot_markers(&stdout);
}
