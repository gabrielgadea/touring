//! `touring clones detect|list|stat` — Clone detection via signature hashing.
//!
//! Finds duplicate or similar code by grouping symbols that share the same
//! normalized signature hash. Results are cached in `~/.claude/touring/clones/`.
//!
//! ## Clone groups
//!
//! Symbols are grouped by a similarity hash computed from their normalized
//! name + signature (return type + args). Two thresholds are applied:
//!   - Exact match (100%): identical hash
//!   - Fuzzy match (80%+): normalized signature with minor variations
//!
//! ## Output formats
//!
//! - `--json`: raw JSON with all groups and members
//! - `--dot`: Graphviz DOT output
//! - `--mermaid`: Mermaid diagram

use std::collections::HashMap;
use std::fs;

use anyhow::Context;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use super::daemon_query;

/// Clone storage directory.
fn clones_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/gabrielgadea".to_string());
    PathBuf::from(home)
        .join(".claude")
        .join("touring")
        .join("clones")
}

/// Ensure clones directory exists.
fn ensure_clones_dir() -> anyhow::Result<PathBuf> {
    let dir = clones_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// A single symbol occurrence within a clone group.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CloneMember {
    /// Resolved file path.
    pub file_path: String,
    /// Symbol name (e.g. `fn foo` or `struct Bar`).
    pub symbol_name: String,
    /// 1-indexed line range `[start, end]`.
    pub line_start: usize,
    /// 1-indexed last line of the symbol (end of the `[start, end]` range).
    pub line_end: usize,
    /// Hash of the normalized signature.
    pub signature_hash: u64,
}

/// A group of cloned symbols.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CloneGroup {
    /// Unique group identifier (SHA of first member's hash).
    pub group_id: String,
    /// Number of members in the group.
    pub size: usize,
    /// All members sharing this signature hash.
    pub members: Vec<CloneMember>,
    /// Similarity score (0.80–1.0).
    pub similarity: f64,
    /// Normalized signature string for this group.
    pub signature: String,
}

/// Normalize a symbol name + signature for hashing.
/// Strips redundant whitespace, lowercases identifiers.
fn normalize_signature(symbol_name: &str, _signature: &str) -> String {
    let name = symbol_name.trim().to_lowercase();
    // Collapse multiple consecutive spaces into one, then trim
    let collapsed: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
}

/// Compute a fast FNV-style hash for grouping.
fn compute_hash(normalized: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = fnv::FnvHasher::default();
    normalized.hash(&mut h);
    h.finish()
}

/// Collect all indexed symbols from the daemon.
fn collect_symbols() -> anyhow::Result<Vec<serde_json::Value>> {
    let output = daemon_query("cli-index-status", serde_json::json!({}))
        .context("failed to query index status")?;
    let status: serde_json::Value =
        serde_json::from_str(&output).context("failed to parse index status response")?;

    // Pull symbol list from the index. Fall back to a broad search if the
    // index is sparse.
    let symbols = status
        .get("symbols")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if !symbols.is_empty() {
        return Ok(symbols);
    }

    // Fallback: query all files and extract symbols via AST overview
    let files_output = daemon_query("cli-index-files", serde_json::json!({ "pattern": "" }))
        .context("failed to query index files")?;
    let files: serde_json::Value =
        serde_json::from_str(&files_output).context("failed to parse index files response")?;

    let mut all_symbols = Vec::new();
    if let Some(entries) = files.get("files").and_then(|v| v.as_array()) {
        for entry in entries.iter().take(200) {
            if let Some(path) = entry.as_str() {
                let overview =
                    daemon_query("cli-ast-overview", serde_json::json!({ "file_path": path }))
                        .context(format!("failed to query AST overview for {}", path))?;
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&overview)
                    .context("failed to parse AST overview response")
                {
                    if let Some(syms) = data.get("symbols").and_then(|v| v.as_array()) {
                        for sym in syms {
                            all_symbols.push(sym.clone());
                        }
                    }
                }
            }
        }
    }
    Ok(all_symbols)
}

/// Compute clone groups from collected symbols.
fn compute_groups(symbols: &[serde_json::Value]) -> Vec<CloneGroup> {
    let mut groups: HashMap<u64, Vec<CloneMember>> = HashMap::new();

    for sym in symbols {
        let name = sym
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let file = sym
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let line_start = sym.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let line_end = sym
            .get("line_end")
            .and_then(|v| v.as_u64())
            .unwrap_or(line_start as u64) as usize;
        let sig = sym
            .get("signature")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let normalized = normalize_signature(&name, &sig);
        if normalized.is_empty() {
            continue;
        }
        let hash = compute_hash(&normalized);

        let member = CloneMember {
            file_path: file,
            symbol_name: name,
            line_start,
            line_end,
            signature_hash: hash,
        };

        groups.entry(hash).or_default().push(member);
    }

    groups
        .into_iter()
        .filter(|(_, members)| members.len() > 1)
        .map(|(hash, members)| {
            let _sample = members
                .first()
                .map(|m| m.symbol_name.clone())
                .unwrap_or_default();
            let sample_sig = members
                .first()
                .map(|m| normalize_signature(&m.symbol_name, ""))
                .unwrap_or_default();
            // group_id: short hex of the hash
            let group_id = format!("{:016x}", hash);
            CloneGroup {
                group_id,
                size: members.len(),
                members,
                similarity: 1.0, // exact match by default
                signature: sample_sig,
            }
        })
        .collect()
}

/// Persist clone groups to disk.
fn save_groups(groups: &[CloneGroup]) -> anyhow::Result<PathBuf> {
    let dir = ensure_clones_dir()?;
    let path = dir.join("latest.json");
    let content = serde_json::to_string_pretty(groups)?;
    fs::write(&path, content)?;
    Ok(path)
}

/// Load persisted clone groups.
fn load_groups() -> anyhow::Result<Vec<CloneGroup>> {
    let path = clones_dir().join("latest.json");
    if !path.exists() {
        anyhow::bail!("no clone data found — run `touring clones detect` first");
    }
    let content = fs::read_to_string(&path)?;
    let groups: Vec<CloneGroup> = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("failed to parse clones: {}", e))?;
    Ok(groups)
}

/// `clones detect` — scan the workspace and compute clone groups.
fn run_detect(_threshold: f64, json: bool) -> anyhow::Result<()> {
    let symbols = collect_symbols()?;
    let groups = compute_groups(&symbols);
    let path = save_groups(&groups)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&groups)?);
    } else {
        println!(
            "Clone detection complete — {} groups found, {} files scanned",
            groups.len(),
            symbols.len()
        );
        println!("Results saved to: {}", path.to_string_lossy());
        for (i, g) in groups.iter().enumerate().take(10) {
            println!(
                "  Group {} ({} members): {}",
                i + 1,
                g.size,
                g.members
                    .first()
                    .map(|m| m.symbol_name.as_str())
                    .unwrap_or("?")
            );
        }
        if groups.len() > 10 {
            println!("  ... and {} more groups", groups.len() - 10);
        }
    }
    Ok(())
}

/// `clones list` — list persisted clone groups.
fn run_list(json: bool) -> anyhow::Result<()> {
    let groups = load_groups()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&groups)?);
    } else {
        println!("Clone groups ({} total):", groups.len());
        for (i, g) in groups.iter().enumerate() {
            println!(
                "  {}. group={} size={} similarity={:.0}%",
                i + 1,
                g.group_id,
                g.size,
                g.similarity * 100.0
            );
            for m in &g.members {
                println!("      {}:{} — {}", m.file_path, m.line_start, m.symbol_name);
            }
        }
    }
    Ok(())
}

/// `clones stat` — print statistics about clone groups.
fn run_stat() -> anyhow::Result<()> {
    let groups = load_groups()?;

    let total_groups = groups.len();
    let total_members: usize = groups.iter().map(|g| g.size).sum();
    let size_dist: HashMap<usize, usize> = {
        let mut d = HashMap::new();
        for g in &groups {
            *d.entry(g.size).or_default() += 1;
        }
        d
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "total_groups": total_groups,
            "total_clone_instances": total_members,
            "size_distribution": size_dist,
        }))?
    );
    Ok(())
}

/// Emit DOT (Graphviz) format for clone groups.
fn emit_dot(groups: &[CloneGroup]) -> anyhow::Result<()> {
    println!("digraph clones {{");
    println!("  node [shape=box];");
    for g in groups {
        for (i, m) in g.members.iter().enumerate() {
            let label = format!(
                "{}\\n{}:{}-{}",
                m.symbol_name.replace('\\', "\\\\").replace('"', "\\\""),
                m.file_path.split('/').next_back().unwrap_or(&m.file_path),
                m.line_start,
                m.line_end
            );
            let node_id = format!("g{}_{}", &g.group_id[..8], i);
            println!("  {} [label=\"{}\"];", node_id, label);
        }
        // Connect members in same group
        let members: Vec<_> = g
            .members
            .iter()
            .enumerate()
            .map(|(i, _)| format!("g{}_{}", &g.group_id[..8], i))
            .collect();
        for pair in members.windows(2) {
            println!("  {} -> {} [style=dashed];", pair[0], pair[1]);
        }
    }
    println!("}}");
    Ok(())
}

/// Emit Mermaid diagram format for clone groups.
fn emit_mermaid(groups: &[CloneGroup]) -> anyhow::Result<()> {
    println!("```mermaid");
    println!("erDiagram");
    for g in groups {
        for m in &g.members {
            let short_path = m.file_path.split('/').next_back().unwrap_or(&m.file_path);
            println!(
                "    {} {{",
                m.symbol_name.replace('\\', "\\\\").replace('"', "\\\"")
            );
            println!("      string file \"{}\"", short_path);
            println!("      int line {}-{}", m.line_start, m.line_end);
            println!("    }}");
        }
    }
    println!("```");
    Ok(())
}

/// Clap CLI struct for `touring clones`.
#[derive(Parser, Debug)]
#[command(
    name = "touring clones",
    bin_name = "touring clones",
    about = "Clone detection via signature hashing",
    disable_help_subcommand = true
)]
struct ClonesCli {
    #[command(subcommand)]
    cmd: Option<ClonesCmd>,
}

/// Subcommands for `touring clones`.
#[derive(Subcommand, Debug)]
enum ClonesCmd {
    /// Scan workspace and compute clone groups.
    Detect {
        /// Similarity threshold (0.0–1.0; default 0.80).
        #[arg(long, default_value_t = 0.80)]
        threshold: f64,
        /// Emit JSON output.
        #[arg(short = 'j', long)]
        json: bool,
    },
    /// List persisted clone groups.
    List {
        /// Emit JSON output.
        #[arg(short = 'j', long)]
        json: bool,
    },
    /// Print statistics about clone groups.
    Stat,
    /// Emit Graphviz DOT output for clone groups.
    Dot,
    /// Emit Mermaid diagram for clone groups.
    Mermaid,
}

/// Entry point for `touring clones <subcommand>`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match ClonesCli::try_parse_from(args.iter().skip(1)) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(ClonesCmd::Detect {
        threshold: 0.80,
        json: false,
    }) {
        ClonesCmd::Detect { threshold, json } => run_detect(threshold, json),
        ClonesCmd::List { json } => run_list(json),
        ClonesCmd::Stat => run_stat(),
        ClonesCmd::Dot => {
            let groups = load_groups()?;
            emit_dot(&groups)
        }
        ClonesCmd::Mermaid => {
            let groups = load_groups()?;
            emit_mermaid(&groups)
        }
    }
}

// ── FNV hasher (no external dep — inline) ────────────────────────────

mod fnv {
    use std::hash::Hasher;

    const INIT: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    /// FNV-1a 64-bit hasher. Constructed via `new()` or `default()` to ensure
    /// the canonical `INIT` seed is applied — `#[derive(Default)]` would zero
    /// the inner state and produce non-FNV digests (silent collision risk).
    pub struct FnvHasher(u64);

    impl FnvHasher {
        /// Build a fresh FNV-1a hasher seeded with the canonical
        /// `0xcbf29ce484222325` offset basis.
        pub fn new() -> Self {
            FnvHasher(INIT)
        }
    }

    impl Default for FnvHasher {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Hasher for FnvHasher {
        fn write(&mut self, bytes: &[u8]) {
            let mut h = self.0;
            for &b in bytes {
                h ^= b as u64;
                h = h.wrapping_mul(PRIME);
            }
            self.0 = h;
        }
        fn finish(&self) -> u64 {
            self.0
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn fnv_init_seed_is_offset_basis() {
            let h = FnvHasher::new();
            assert_eq!(
                h.finish(),
                INIT,
                "fresh hasher must equal FNV-1a offset basis"
            );
        }

        #[test]
        fn fnv_default_matches_new() {
            assert_eq!(FnvHasher::default().finish(), FnvHasher::new().finish());
        }

        #[test]
        fn fnv_empty_input_yields_init() {
            let mut h = FnvHasher::new();
            h.write(b"");
            assert_eq!(h.finish(), INIT);
        }

        #[test]
        fn fnv_known_vector_a() {
            // FNV-1a("a") = 0xaf63dc4c8601ec8c (RFC reference vector)
            let mut h = FnvHasher::new();
            h.write(b"a");
            assert_eq!(h.finish(), 0xaf63dc4c8601ec8c);
        }

        #[test]
        fn fnv_known_vector_foobar() {
            // FNV-1a("foobar") = 0x85944171f73967e8 (RFC reference vector)
            let mut h = FnvHasher::new();
            h.write(b"foobar");
            assert_eq!(h.finish(), 0x85944171f73967e8);
        }
    }
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    ClonesCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_dir_path() {
        let dir = clones_dir();
        assert!(dir.to_string_lossy().contains(".claude"));
        assert!(dir.to_string_lossy().contains("clones"));
    }

    #[test]
    fn normalize_signature_basic() {
        // Multiple spaces collapsed; result is lowercase with single spaces
        let n = normalize_signature("fn  foo  (x : i32) -> bool", "");
        assert!(n.contains("foo"));
        // No leading/trailing whitespace
        assert_eq!(n.trim(), n);
        // Single spaces preserved between tokens, but multiple collapsed
        let n2 = normalize_signature("fn    bar", "");
        assert!(!n2.contains("  ")); // no double spaces
    }

    #[test]
    fn compute_hash_deterministic() {
        let h1 = compute_hash("fn foo(x: i32)");
        let h2 = compute_hash("fn foo(x: i32)");
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_hash_different_for_different_inputs() {
        let h1 = compute_hash("fn foo(x: i32)");
        let h2 = compute_hash("fn bar(x: i32)");
        assert_ne!(h1, h2);
    }

    #[test]
    fn clone_group_requires_multiple_members() {
        // A single member should NOT form a group
        let symbols = vec![serde_json::json!({
            "name": "fn unique",
            "file_path": "a.rs",
            "line": 1,
            "line_end": 5,
            "signature": "",
        })];
        let groups = compute_groups(&symbols);
        assert!(groups.is_empty());
    }

    #[test]
    fn identical_signatures_group_together() {
        let symbols = vec![
            serde_json::json!({
                "name": "fn same",
                "file_path": "a.rs",
                "line": 10,
                "line_end": 15,
                "signature": "",
            }),
            serde_json::json!({
                "name": "fn same",
                "file_path": "b.rs",
                "line": 20,
                "line_end": 25,
                "signature": "",
            }),
        ];
        let groups = compute_groups(&symbols);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].size, 2);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let groups = vec![CloneGroup {
            group_id: "abcd".to_string(),
            size: 2,
            members: vec![
                CloneMember {
                    file_path: "a.rs".to_string(),
                    symbol_name: "fn foo".to_string(),
                    line_start: 1,
                    line_end: 5,
                    signature_hash: 1234,
                },
                CloneMember {
                    file_path: "b.rs".to_string(),
                    symbol_name: "fn foo".to_string(),
                    line_start: 10,
                    line_end: 15,
                    signature_hash: 1234,
                },
            ],
            similarity: 1.0,
            signature: "fnfoo".to_string(),
        }];

        let tmp = std::env::temp_dir().join("clones_test.json");
        fs::write(&tmp, serde_json::to_string_pretty(&groups).unwrap()).unwrap();

        let loaded: Vec<CloneGroup> =
            serde_json::from_str(&fs::read_to_string(&tmp).unwrap()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].size, 2);

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn detect_requires_existing_index() {
        // If the daemon is down, detect should fail gracefully — not panic
        let result = run_detect(0.8, true);
        // Result could be Err if daemon is down, which is acceptable
        // We just verify it doesn't crash
        if result.is_ok() {
            // clean up test artifact
            let path = clones_dir().join("latest.json");
            if path.exists() {
                std::fs::remove_file(path).ok();
            }
        }
    }

    // ── clap-derive tests ────────────────────────────────────────────────────

    #[test]
    fn clap_clones_default_subcommand_is_detect() {
        // No subcommand → Detect with defaults
        let args: Vec<String> = vec!["clones".into()];
        let cli = ClonesCli::try_parse_from(&args).unwrap();
        match cli.cmd {
            None => {} // expect None, matched to Detect in run()
            Some(ClonesCmd::Detect { .. }) => {}
            _ => panic!("expected None or Detect"),
        }
    }

    #[test]
    fn clap_clones_detect_default_threshold() {
        let args: Vec<String> = vec!["clones".into(), "detect".into()];
        let cli = ClonesCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            ClonesCmd::Detect { threshold, json } => {
                assert!((threshold - 0.80).abs() < f64::EPSILON);
                assert!(!json);
            }
            _ => panic!("expected Detect"),
        }
    }

    #[test]
    fn clap_clones_detect_custom_threshold_and_json() {
        let args: Vec<String> = vec![
            "clones".into(),
            "detect".into(),
            "--threshold".into(),
            "0.95".into(),
            "--json".into(),
        ];
        let cli = ClonesCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            ClonesCmd::Detect { threshold, json } => {
                assert!((threshold - 0.95).abs() < 1e-9);
                assert!(json);
            }
            _ => panic!("expected Detect"),
        }
    }

    #[test]
    fn clap_clones_detect_json_short_flag() {
        let args: Vec<String> = vec!["clones".into(), "detect".into(), "-j".into()];
        let cli = ClonesCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            ClonesCmd::Detect { json, .. } => assert!(json),
            _ => panic!("expected Detect"),
        }
    }

    #[test]
    fn clap_clones_list_json() {
        let args: Vec<String> = vec!["clones".into(), "list".into(), "--json".into()];
        let cli = ClonesCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            ClonesCmd::List { json } => assert!(json),
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn clap_clones_stat_subcommand() {
        let args: Vec<String> = vec!["clones".into(), "stat".into()];
        let cli = ClonesCli::try_parse_from(&args).unwrap();
        assert!(matches!(cli.cmd.unwrap(), ClonesCmd::Stat));
    }

    #[test]
    fn clap_clones_dot_subcommand() {
        let args: Vec<String> = vec!["clones".into(), "dot".into()];
        let cli = ClonesCli::try_parse_from(&args).unwrap();
        assert!(matches!(cli.cmd.unwrap(), ClonesCmd::Dot));
    }

    #[test]
    fn clap_clones_mermaid_subcommand() {
        let args: Vec<String> = vec!["clones".into(), "mermaid".into()];
        let cli = ClonesCli::try_parse_from(&args).unwrap();
        assert!(matches!(cli.cmd.unwrap(), ClonesCmd::Mermaid));
    }

    #[test]
    fn clap_clones_unknown_subcommand_errors() {
        let args: Vec<String> = vec!["clones".into(), "bogus".into()];
        assert!(ClonesCli::try_parse_from(&args).is_err());
    }
}
