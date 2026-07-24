//! Per-root-cause diagnostics aggregated into a single struct.
//!
//! This module reads workspace data structures already populated by the
//! caller (typically the CLI / MCP layer using `touring wiring`,
//! `touring file-knowledge extended`, and `touring ast tdg`) and shapes
//! them into the Sentrux MCP-compatible `diagnostics` field of
//! `crate::types::WorkspaceQualitySignal`.
//!
//! All thresholds are conservative defaults; the rules engine (W2 P3)
//! will allow per-project overrides via `.touring/rules.toml`.

use super::acyclicity::collect_cycle_paths;
use super::depth::longest_path_witness;
use super::types::{
    AcyclicityDiagnostics, DepthDiagnostics, Diagnostics, EqualityDiagnostics, FileFanin,
    FileFanout, FileLines, FuncComplexity, FuncIdent, InstabilityEntry, ModularityDiagnostics,
    RedundancyDiagnostics, Workspace,
};

/// Per-file fan-out threshold above which a file is flagged as a "god file".
pub const GOD_FILE_FANOUT_THRESHOLD: usize = 10;
/// Per-file fan-in threshold above which a file is a hotspot candidate.
pub const HOTSPOT_FANIN_THRESHOLD: usize = 15;
/// Hotspot files must also exceed this Martin instability.
pub const HOTSPOT_INSTABILITY_THRESHOLD: f64 = 0.7;
/// Cyclomatic complexity above which a function is "complex" for diagnostics.
pub const COMPLEX_FN_CC_THRESHOLD: u32 = 15;
/// File size above which a file is flagged as "large" for diagnostics.
pub const LARGE_FILE_LINES_THRESHOLD: usize = 500;
/// Number of unstable files included in the most-unstable list.
pub const UNSTABLE_TOP_N: usize = 10;

/// Collect all per-root-cause diagnostics from `ws`.
#[must_use]
pub fn collect_diagnostics(ws: &Workspace) -> Diagnostics {
    Diagnostics {
        modularity: collect_modularity(ws),
        acyclicity: collect_acyclicity(ws),
        depth: collect_depth(ws),
        equality: collect_equality(ws),
        redundancy: collect_redundancy(ws),
    }
}

fn collect_modularity(ws: &Workspace) -> ModularityDiagnostics {
    let god_files: Vec<FileFanout> = ws
        .file_fan_out
        .iter()
        .filter(|(_, fan)| **fan > GOD_FILE_FANOUT_THRESHOLD)
        .map(|(p, fan)| FileFanout {
            path: p.clone(),
            fan_out: *fan,
        })
        .collect();

    let hotspot_files: Vec<FileFanin> = ws
        .file_fan_in
        .iter()
        .filter_map(|(p, fan_in)| {
            if *fan_in <= HOTSPOT_FANIN_THRESHOLD {
                return None;
            }
            let fan_out = ws.file_fan_out.get(p).copied().unwrap_or(0);
            let instability = martin_instability(*fan_in, fan_out);
            (instability > HOTSPOT_INSTABILITY_THRESHOLD).then_some(FileFanin {
                path: p.clone(),
                fan_in: *fan_in,
                instability,
            })
        })
        .collect();

    let mut most_unstable: Vec<InstabilityEntry> = ws
        .file_fan_in
        .keys()
        .chain(ws.file_fan_out.keys())
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .map(|p| {
            let fan_in = ws.file_fan_in.get(p).copied().unwrap_or(0);
            let fan_out = ws.file_fan_out.get(p).copied().unwrap_or(0);
            InstabilityEntry {
                path: p.to_string(),
                instability: martin_instability(fan_in, fan_out),
                fan_in,
                fan_out,
            }
        })
        .collect();
    most_unstable.sort_by(|a, b| {
        b.instability
            .partial_cmp(&a.instability)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    most_unstable.truncate(UNSTABLE_TOP_N);

    ModularityDiagnostics {
        god_files,
        hotspot_files,
        most_unstable,
    }
}

fn collect_acyclicity(ws: &Workspace) -> AcyclicityDiagnostics {
    AcyclicityDiagnostics {
        cycle_paths: collect_cycle_paths(ws),
    }
}

fn collect_depth(ws: &Workspace) -> DepthDiagnostics {
    DepthDiagnostics {
        longest_path_witness: longest_path_witness(ws),
    }
}

fn collect_equality(ws: &Workspace) -> EqualityDiagnostics {
    let complex_functions: Vec<FuncComplexity> = ws
        .function_cc
        .iter()
        .filter(|f| f.cc > COMPLEX_FN_CC_THRESHOLD)
        .cloned()
        .collect();
    let large_files: Vec<FileLines> = ws
        .file_lines
        .iter()
        .filter_map(|(p, n)| {
            (*n > LARGE_FILE_LINES_THRESHOLD).then_some(FileLines {
                path: p.clone(),
                lines: *n,
            })
        })
        .collect();
    EqualityDiagnostics {
        complex_functions,
        large_files,
    }
}

fn collect_redundancy(ws: &Workspace) -> RedundancyDiagnostics {
    RedundancyDiagnostics {
        dead_functions: ws
            .dead_functions
            .iter()
            .map(|f| FuncIdent {
                file: f.file.clone(),
                func: f.func.clone(),
            })
            .collect(),
        duplicate_groups: ws.duplicate_groups.clone(),
    }
}

/// Martin instability `I = Ce / (Ca + Ce)`. Returns `0` when both are zero.
#[must_use]
pub fn martin_instability(fan_in: usize, fan_out: usize) -> f64 {
    let total = fan_in + fan_out;
    if total == 0 {
        return 0.0;
    }
    (fan_out as f64) / (total as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        let mut ws = Workspace::empty("/tmp");
        ws.file_fan_out.insert("god.rs".into(), 20);
        ws.file_fan_out.insert("normal.rs".into(), 3);
        ws.file_fan_in.insert("hotspot.rs".into(), 25);
        ws.file_fan_out.insert("hotspot.rs".into(), 2); // I = 2/27 ≈ 0.07 → NOT hotspot
        ws.file_fan_in.insert("realhot.rs".into(), 25);
        ws.file_fan_out.insert("realhot.rs".into(), 80); // I = 80/105 ≈ 0.76 → hotspot
        ws.function_cc.push(FuncComplexity {
            file: "a.rs".into(),
            func: "complex".into(),
            cc: 30,
        });
        ws.function_cc.push(FuncComplexity {
            file: "b.rs".into(),
            func: "simple".into(),
            cc: 5,
        });
        ws.file_lines.insert("big.rs".into(), 1200);
        ws.file_lines.insert("small.rs".into(), 50);
        ws.dead_functions.push(FuncIdent {
            file: "x.rs".into(),
            func: "unused".into(),
        });
        ws
    }

    #[test]
    fn god_files_detected_above_threshold() {
        let d = collect_modularity(&ws());
        assert!(d.god_files.iter().any(|f| f.path == "god.rs"));
        assert!(!d.god_files.iter().any(|f| f.path == "normal.rs"));
    }

    #[test]
    fn hotspots_require_high_instability() {
        let d = collect_modularity(&ws());
        assert!(d.hotspot_files.iter().any(|f| f.path == "realhot.rs"));
        assert!(!d.hotspot_files.iter().any(|f| f.path == "hotspot.rs"));
    }

    #[test]
    fn complex_functions_above_cc_threshold() {
        let d = collect_equality(&ws());
        assert_eq!(d.complex_functions.len(), 1);
        assert_eq!(d.complex_functions[0].func, "complex");
    }

    #[test]
    fn large_files_above_lines_threshold() {
        let d = collect_equality(&ws());
        assert_eq!(d.large_files.len(), 1);
        assert_eq!(d.large_files[0].path, "big.rs");
    }

    #[test]
    fn dead_functions_propagated() {
        let d = collect_redundancy(&ws());
        assert_eq!(d.dead_functions.len(), 1);
        assert_eq!(d.dead_functions[0].func, "unused");
    }

    #[test]
    fn martin_instability_endpoints() {
        assert!(martin_instability(0, 0).abs() < f64::EPSILON);
        assert!((martin_instability(10, 0) - 0.0).abs() < f64::EPSILON);
        assert!((martin_instability(0, 10) - 1.0).abs() < f64::EPSILON);
        assert!((martin_instability(5, 5) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn collect_diagnostics_aggregates_all_five_dims() {
        let d = collect_diagnostics(&ws());
        assert!(!d.modularity.god_files.is_empty());
        assert!(d.acyclicity.cycle_paths.is_empty()); // no cycles in fixture
        assert!(!d.equality.complex_functions.is_empty());
        assert!(!d.equality.large_files.is_empty());
        assert!(!d.redundancy.dead_functions.is_empty());
    }
}
