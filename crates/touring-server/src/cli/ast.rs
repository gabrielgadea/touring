//! `touring ast find|overview|blast|...` — AST query and blast radius analysis.
//!
//! Provides AST-level symbol lookup, file overview, and impact analysis.
//!
//! Wave P3-1.3 W5a (2026-06-11): migrated from manual `arg_or` + `flag_value`
//! positional parsing to clap derive. The dispatch contract is unchanged —
//! `run` still receives the full argv slice; clap parses `args[1..]`.
//!
//! G6 payload byte-compatible: every `daemon_query` call keeps exactly the
//! same hook name and JSON keys as the original `arg_or` implementation.
//! Pure-library subcommands (rust-semantic, format-rust, workspace-info,
//! node-types, importance, workflow, highlight) are preserved verbatim.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring ast",
    bin_name = "touring ast",
    about = "AST analysis: find (default), overview, blast, semantic, quality, tdg, scan, calls, heat, modules, scope, detail, imports, callgraph, todos, rationale, features, meta, skeleton, blast-enriched, blast-cross-feature, rust-semantic, polyglot-semantic, format-rust, workspace-info, node-types, importance, grep, highlight, workflow",
    disable_help_subcommand = true
)]
struct AstCli {
    #[command(subcommand)]
    cmd: Option<AstCmd>,
}

#[derive(Subcommand, Debug)]
enum AstCmd {
    /// Find a symbol by name (default subcommand).
    Find {
        /// Symbol name to look up.
        #[arg(default_value = "")]
        symbol_name: String,
        /// Return definitions only.
        #[arg(long)]
        definitions_only: bool,
        /// Optional file path to restrict search.
        #[arg(long)]
        file_path: Option<String>,
    },
    /// Show file overview (symbols, imports, structure).
    Overview {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// Show blast radius for a file.
    Blast {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
        /// Render as termtree instead of raw JSON.
        #[arg(long)]
        tree: bool,
    },
    /// Semantic vector search.
    Semantic {
        /// Query string.
        #[arg(default_value = "")]
        query: String,
        /// Similarity threshold (0.0-1.0, default: 0.5).
        #[arg(long, default_value_t = 0.5f64)]
        threshold: f64,
        /// Maximum results (default: 10).
        #[arg(long, default_value_t = 10u64)]
        limit: u64,
    },
    /// Quality metrics for a file.
    Quality {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// TDG (Technical Debt Grade) for a file.
    Tdg {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
        /// Emit grade letter only.
        #[arg(long)]
        grade_only: bool,
    },
    /// Batch structural scan with YAML rules.
    Scan {
        /// Path to rules directory.
        #[arg(long, default_value = "")]
        rules: String,
        /// Root directory to scan.
        #[arg(long, default_value = "")]
        root: String,
    },
    /// Call-site analysis for a symbol.
    Calls {
        /// Symbol name.
        #[arg(default_value = "")]
        symbol: String,
        /// Optional file path to restrict search.
        #[arg(long, default_value = "")]
        file: String,
    },
    /// Heat-map analysis for a file.
    Heat {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// List modules in a directory.
    Modules {
        /// Directory path.
        #[arg(default_value = "")]
        dir: String,
    },
    /// Scope analysis for a file.
    Scope {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// Detailed symbol information.
    Detail {
        /// Symbol name.
        #[arg(default_value = "")]
        symbol: String,
        /// Optional file path.
        #[arg(long, default_value = "")]
        file: String,
    },
    /// Import analysis for a file.
    Imports {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// Call graph for a file.
    Callgraph {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// TODO/FIXME extraction for a file.
    Todos {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// Design rationale extraction for a file.
    Rationale {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// Feature-gate analysis for a file.
    Features {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// File metadata at requested depth (skeleton/summary/full).
    Meta {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
        /// Depth: skeleton, summary, or full (default: skeleton).
        #[arg(long, default_value = "skeleton")]
        depth: String,
    },
    /// AST skeleton (structural outline only).
    Skeleton {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// Enriched blast radius.
    BlastEnriched {
        /// File path to analyse.
        #[arg(default_value = "")]
        file_path: String,
    },
    /// Cross-feature blast radius.
    BlastCrossFeature {
        /// File path to analyse (flags are silently ignored).
        #[arg(default_value = "")]
        file_path: String,
    },
    /// Deep Rust semantic analysis via syn (no daemon).
    RustSemantic {
        /// Rust source file path.
        file_path: String,
    },
    /// Deep polyglot semantic analysis via tree-sitter — the cross-language
    /// analog of rust-semantic for Python/TypeScript/JavaScript (no daemon).
    PolyglotSemantic {
        /// Source file path (.py/.ts/.tsx/.js/.jsx); language inferred from extension.
        file_path: String,
    },
    /// Format a Rust source file via prettyplease (no daemon).
    FormatRust {
        /// Preserve comments during formatting.
        #[arg(long)]
        preserve: bool,
        /// Rust source file path.
        file_path: String,
    },
    /// Workspace-wide feature/dependency analysis via cargo_metadata (no daemon).
    WorkspaceInfo {
        /// Directory containing Cargo.toml (default: current directory).
        #[arg(default_value = ".")]
        dir: String,
    },
    /// List AST node types for a language (no daemon).
    NodeTypes {
        /// Language name (default: rust).
        #[arg(default_value = "rust")]
        lang: String,
    },
    /// Filter node types by importance threshold (no daemon).
    Importance {
        /// Rust source file path.
        file_path: String,
        /// Importance threshold 0.0-1.0 (default: 0.5).
        #[arg(long, default_value_t = 0.5f64)]
        threshold: f64,
    },
    /// Polyglot structural search + rewrite via ast-grep.
    Grep {
        /// File path to search.
        file_path: String,
        /// ast-grep pattern (metavars: $VAR / $$$VAR).
        pattern: String,
        /// Optional rewrite template.
        #[arg(long)]
        rewrite: Option<String>,
        /// Language override (rust, typescript, python, …).
        #[arg(long)]
        lang: Option<String>,
        /// Maximum results (default: 50).
        #[arg(long, default_value_t = 50u64)]
        top: u64,
        /// Skip matches inside string literals.
        #[arg(long)]
        skip_strings: bool,
    },
    /// Syntect-based ANSI syntax highlighter (no daemon, delegates to highlight module).
    Highlight {
        /// Remaining args passed through to highlight::run.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// One-shot code-gen workflow helper (rust-semantic + public-api + format-rust, no daemon).
    Workflow {
        /// Rust source file path.
        file_path: String,
    },
}

/// `touring ast workspace-info` helper — Cargo view via `cargo_metadata` (may be
/// absent on a non-Rust project) PLUS the multi-ecosystem manifest inventory
/// (npm/PyPI/Go). P-E toolchain parity: `workspace-info` no longer fails on a
/// Python/Node/Go tree. Output is backward-compatible — the flat Cargo fields
/// stay at the top level and a new `manifests` array carries the non-Cargo view.
fn run_workspace_info(dir: &str) -> anyhow::Result<()> {
    let cargo = touring_code::ast::WorkspaceInfo::load(dir).ok();
    let inventory = touring_code::ast::manifest::ManifestInventory::scan(dir);
    if cargo.is_none() && inventory.is_empty() {
        anyhow::bail!(
            "no Cargo/npm/PyPI/Go manifest at or under {dir} \
             (looked for Cargo.toml, package.json, pyproject.toml, go.mod)"
        );
    }
    let mut json = match &cargo {
        Some(w) => serde_json::to_value(w)?,
        None => serde_json::json!({
            "workspace_root": inventory.root,
            "packages": [],
            "workspace_member_count": 0,
        }),
    };
    json["manifests"] = serde_json::to_value(&inventory.manifests)?;
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

/// Run the `ast` CLI subcommand dispatcher.
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable, the daemon reports
/// a failure, or required arguments are missing / files cannot be read.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    // `-j`/`--json` accepted for compatibility (clap-derive migration W5a dropped
    // the legacy global flag); ast subcommands emit JSON by default, so strip it.
    let args: Vec<String> = args
        .iter()
        .filter(|a| a.as_str() != "-j" && a.as_str() != "--json")
        .cloned()
        .collect();
    let cli = match AstCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(AstCmd::Find {
        symbol_name: String::new(),
        definitions_only: false,
        file_path: None,
    }) {
        AstCmd::Find {
            symbol_name,
            definitions_only,
            file_path,
        } => {
            let payload = serde_json::json!({
                "symbol_name": symbol_name,
                "definitions_only": definitions_only,
                "file_path": file_path,
            });
            let output = daemon_query("cli-ast-find", payload)?;
            println!("{output}");
        }
        AstCmd::Overview { file_path } => {
            let payload = serde_json::json!({ "file_path": file_path });
            let output = daemon_query("cli-ast-overview", payload)?;
            println!("{output}");
        }
        AstCmd::Blast { file_path, tree } => {
            let payload = serde_json::json!({ "file_path": file_path });
            let output = daemon_query("cli-ast-blast", payload)?;
            if tree {
                println!("{}", render_blast_as_tree(&output));
            } else {
                println!("{output}");
            }
        }
        AstCmd::Semantic {
            query,
            threshold,
            limit,
        } => {
            let payload =
                serde_json::json!({"query": query, "threshold": threshold, "limit": limit});
            let output = daemon_query("cli-ast-semantic", payload)?;
            println!("{output}");
        }
        AstCmd::Quality { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-quality", payload)?;
            println!("{output}");
        }
        AstCmd::Tdg {
            file_path,
            grade_only,
        } => {
            let payload = serde_json::json!({
                "file_path": file_path,
                "grade_only": grade_only,
            });
            let output = daemon_query("cli-ast-tdg", payload)?;
            println!("{output}");
        }
        AstCmd::Scan { rules, root } => {
            let mut payload = serde_json::json!({"rules_dir": rules});
            if !root.is_empty() {
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("root".to_string(), serde_json::json!(root));
                }
            }
            let output = daemon_query("cli-ast-scan", payload)?;
            println!("{output}");
        }
        AstCmd::Calls { symbol, file } => {
            let payload = serde_json::json!({"symbol": symbol, "file_path": file});
            let output = daemon_query("cli-ast-calls", payload)?;
            println!("{output}");
        }
        AstCmd::Heat { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-heat", payload)?;
            println!("{output}");
        }
        AstCmd::Modules { dir } => {
            let payload = serde_json::json!({"dir": dir});
            let output = daemon_query("cli-ast-modules", payload)?;
            println!("{output}");
        }
        AstCmd::Scope { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-scope", payload)?;
            println!("{output}");
        }
        AstCmd::Detail { symbol, file } => {
            let payload = serde_json::json!({"symbol": symbol, "file_path": file});
            let output = daemon_query("cli-ast-detail", payload)?;
            println!("{output}");
        }
        AstCmd::Imports { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-imports", payload)?;
            println!("{output}");
        }
        AstCmd::Callgraph { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-callgraph", payload)?;
            println!("{output}");
        }
        AstCmd::Todos { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-todos", payload)?;
            println!("{output}");
        }
        AstCmd::Rationale { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-rationale", payload)?;
            println!("{output}");
        }
        AstCmd::Features { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-features", payload)?;
            println!("{output}");
        }
        AstCmd::Meta { file_path, depth } => {
            let payload = serde_json::json!({"file_path": file_path, "depth": depth});
            let output = daemon_query("cli-ast-meta", payload)?;
            println!("{output}");
        }
        AstCmd::Skeleton { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-skeleton", payload)?;
            println!("{output}");
        }
        AstCmd::BlastEnriched { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-blast-enriched", payload)?;
            println!("{output}");
        }
        AstCmd::BlastCrossFeature { file_path } => {
            let payload = serde_json::json!({"file_path": file_path});
            let output = daemon_query("cli-ast-blast-cross-feature", payload)?;
            println!("{output}");
        }
        // ── Pure library calls — no daemon ────────────────────────────────────
        AstCmd::RustSemantic { file_path } => {
            if file_path.is_empty() {
                anyhow::bail!(
                    "rust-semantic requires a file path: touring ast rust-semantic <file.rs>"
                );
            }
            let source = std::fs::read_to_string(&file_path)
                .map_err(|e| anyhow::anyhow!("failed to read {file_path}: {e}"))?;
            let report = touring_code::ast::rust_semantic::RustSemanticReport::from_source(&source)
                .map_err(|e| anyhow::anyhow!("parse failed: {e}"))?;
            let json = serde_json::json!({
                "file_path": file_path,
                "item_count": report.item_count,
                "generics": report.generics,
                "trait_impls": report.trait_impls,
                "lifetimes": report.lifetimes,
                "derives": report.derives,
                "where_clauses": report.where_clauses,
                "unsafe_blocks": report.unsafe_blocks,
                "async_fns": report.async_fns,
                "total_trait_bounds": report.total_trait_bounds(),
                "semantic_complexity": report.semantic_complexity(),
                "is_simple": report.is_simple(),
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        AstCmd::PolyglotSemantic { file_path } => {
            if file_path.is_empty() {
                anyhow::bail!(
                    "polyglot-semantic requires a file path: \
                     touring ast polyglot-semantic <file.py|.ts|.js>"
                );
            }
            let lang =
                touring_code::ast::languages::Lang::from_path(std::path::Path::new(&file_path))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "unknown language for {file_path} (expected .py/.ts/.tsx/.js/.jsx)"
                        )
                    })?;
            let source = std::fs::read_to_string(&file_path)
                .map_err(|e| anyhow::anyhow!("failed to read {file_path}: {e}"))?;
            let report =
                touring_code::ast::polyglot_semantic::PolyglotSemanticReport::from_source(
                    lang, &source,
                )
                .map_err(|e| anyhow::anyhow!("parse failed: {e}"))?;
            let json = serde_json::json!({
                "file_path": file_path,
                "language": report.language,
                "item_count": report.item_count,
                "type_params": report.type_params,
                "async_fns": report.async_fns,
                "decorators": report.decorators,
                "classes": report.classes,
                "functions": report.functions,
                "typed_params": report.typed_params,
                "total_params": report.total_params,
                "dynamic_escapes": report.dynamic_escapes,
                "annotation_coverage": report.annotation_coverage(),
                "semantic_complexity": report.semantic_complexity(),
                "is_simple": report.is_simple(),
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        AstCmd::FormatRust {
            preserve,
            file_path,
        } => {
            if file_path.is_empty() {
                anyhow::bail!(
                    "format-rust requires a file path: touring ast format-rust [--preserve] <file.rs>"
                );
            }
            let source = std::fs::read_to_string(&file_path)
                .map_err(|e| anyhow::anyhow!("failed to read {file_path}: {e}"))?;
            let formatted = if preserve {
                touring_code::ast::format_preserve(&source)
                    .map_err(|e| anyhow::anyhow!("preserve format failed: {e}"))?
            } else {
                touring_code::ast::format_rust_code(&source)
                    .map_err(|e| anyhow::anyhow!("format failed: {e}"))?
            };
            print!("{formatted}");
        }
        AstCmd::WorkspaceInfo { dir } => run_workspace_info(&dir)?,
        AstCmd::NodeTypes { lang } => {
            let result = touring_code::ast::node_types_for_language(&lang);
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        AstCmd::Importance {
            file_path,
            threshold,
        } => {
            if file_path.is_empty() {
                anyhow::bail!(
                    "importance requires a file path: touring ast importance <file.rs> --threshold <0.0-1.0>"
                );
            }
            let lang =
                touring_code::ast::languages::Lang::from_path(std::path::Path::new(&file_path))
                    .unwrap_or(touring_code::ast::languages::Lang::Rust);
            let node_types = touring_code::ast::node_types_for_language(lang.as_str());
            let filtered =
                touring_code::ast::importance_threshold(&node_types.node_types, threshold);
            let result = serde_json::json!({
                "file_path": file_path,
                "language": lang.as_str(),
                "threshold": threshold,
                "node_count": filtered.len(),
                "node_types": filtered
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        AstCmd::Grep {
            file_path,
            pattern,
            rewrite,
            lang,
            top,
            skip_strings,
        } => {
            if file_path.is_empty() || pattern.is_empty() {
                anyhow::bail!(
                    "grep requires file and pattern: touring ast grep <file> <pattern> [--rewrite <r>] [--lang <name>] [--top N] [--skip-strings]"
                );
            }
            let mut payload = serde_json::json!({
                "file_path": file_path,
                "pattern": pattern,
                "top": top,
            });
            if let Some(r) = rewrite {
                payload["rewrite"] = serde_json::Value::String(r);
            }
            if let Some(l) = lang {
                payload["lang"] = serde_json::Value::String(l);
            }
            if skip_strings {
                payload["skip_strings"] = serde_json::Value::Bool(true);
            }
            let output = daemon_query("cli-ast-grep", payload)?;
            println!("{output}");
        }
        AstCmd::Highlight { args: sub_args } => {
            // `highlight::run` eats TWO leading tokens before the positional file:
            // it calls `try_parse_from(args.iter().skip(1))` — the `.skip(1)` drops
            // the first token, and clap's `try_parse_from` then consumes its own
            // argv[0]. So the file must be the THIRD element. The original
            // 3-token prefix ["touring","ast","highlight"] left "highlight" + file
            // as two positionals ("unexpected argument"); a 1-token prefix left the
            // file as clap's argv[0] ("required argument not provided"). Exactly two
            // filler tokens puts the file in the positional slot. (A3)
            let mut full_args = vec!["touring".to_string(), "highlight".to_string()];
            full_args.extend(sub_args);
            return super::highlight::run(&full_args);
        }
        AstCmd::Workflow { file_path } => {
            let source = std::fs::read_to_string(&file_path)
                .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", file_path))?;
            let report = touring_code::ast::CodeGenWorkflow::analyze(&source)
                .map_err(|e| anyhow::anyhow!("CodeGenWorkflow::analyze failed: {e}"))?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}

/// Render a `cli-ast-blast` JSON response as a termtree tree.
///
/// Falls back to raw JSON if the input cannot be parsed.
fn render_blast_as_tree(json_output: &str) -> String {
    use termtree::Tree;
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json_output) else {
        return json_output.to_string();
    };
    let file_path = v.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
    let blast_radius = v.get("blast_radius").and_then(|v| v.as_u64()).unwrap_or(0);
    let mut root = Tree::new(format!("{file_path} [blast_radius={blast_radius}]"));

    if let Some(consumers) = v.get("consumers").and_then(|v| v.as_array()) {
        if !consumers.is_empty() {
            let mut deps = Tree::new("Direct dependents".to_string());
            for c in consumers {
                if let Some(s) = c.as_str() {
                    deps.push(Tree::new(s.to_string()));
                }
            }
            root.push(deps);
        }
    }

    if let Some(coedit) = v.get("coedit_files").and_then(|v| v.as_array()) {
        if !coedit.is_empty() {
            let mut co = Tree::new("Co-edit signals".to_string());
            for c in coedit {
                if let Some(s) = c.as_str() {
                    co.push(Tree::new(s.to_string()));
                }
            }
            root.push(co);
        }
    }

    format!("{root}")
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    AstCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> AstCli {
        AstCli::try_parse_from(args).expect("args should parse")
    }

    // ── defaults ──────────────────────────────────────────────────────────────

    #[test]
    fn bare_ast_defaults_to_find() {
        let cli = parse(&["ast"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Find with empty symbol
    }

    // ── subcommand routing ────────────────────────────────────────────────────

    #[test]
    fn find_parses_symbol_name() {
        let cli = parse(&["ast", "find", "HookRuntime"]);
        let Some(AstCmd::Find {
            symbol_name,
            definitions_only,
            file_path,
        }) = cli.cmd
        else {
            panic!("expected Find");
        };
        assert_eq!(symbol_name, "HookRuntime");
        assert!(!definitions_only);
        assert_eq!(file_path, None);
    }

    #[test]
    fn find_definitions_only_flag() {
        let cli = parse(&["ast", "find", "Foo", "--definitions-only"]);
        let Some(AstCmd::Find {
            definitions_only, ..
        }) = cli.cmd
        else {
            panic!("expected Find");
        };
        assert!(definitions_only);
    }

    #[test]
    fn find_with_file_path_flag() {
        let cli = parse(&["ast", "find", "Bar", "--file-path", "src/lib.rs"]);
        let Some(AstCmd::Find { file_path, .. }) = cli.cmd else {
            panic!("expected Find");
        };
        assert_eq!(file_path.as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn overview_parses_file_path() {
        let cli = parse(&["ast", "overview", "src/main.rs"]);
        let Some(AstCmd::Overview { file_path }) = cli.cmd else {
            panic!("expected Overview");
        };
        assert_eq!(file_path, "src/main.rs");
    }

    #[test]
    fn blast_parses_file_path_and_tree_flag() {
        let cli = parse(&["ast", "blast", "src/main.rs", "--tree"]);
        let Some(AstCmd::Blast { file_path, tree }) = cli.cmd else {
            panic!("expected Blast");
        };
        assert_eq!(file_path, "src/main.rs");
        assert!(tree);
    }

    #[test]
    fn blast_defaults_tree_to_false() {
        let cli = parse(&["ast", "blast", "src/main.rs"]);
        let Some(AstCmd::Blast { tree, .. }) = cli.cmd else {
            panic!("expected Blast");
        };
        assert!(!tree);
    }

    #[test]
    fn semantic_parses_query_and_flags() {
        let cli = parse(&[
            "ast",
            "semantic",
            "HookRuntime",
            "--threshold",
            "0.8",
            "--limit",
            "5",
        ]);
        let Some(AstCmd::Semantic {
            query,
            threshold,
            limit,
        }) = cli.cmd
        else {
            panic!("expected Semantic");
        };
        assert_eq!(query, "HookRuntime");
        assert!((threshold - 0.8).abs() < 1e-9);
        assert_eq!(limit, 5);
    }

    #[test]
    fn semantic_defaults() {
        let cli = parse(&["ast", "semantic", "query"]);
        let Some(AstCmd::Semantic {
            threshold, limit, ..
        }) = cli.cmd
        else {
            panic!("expected Semantic");
        };
        assert!((threshold - 0.5).abs() < 1e-9);
        assert_eq!(limit, 10);
    }

    #[test]
    fn tdg_grade_only_flag() {
        let cli = parse(&["ast", "tdg", "src/lib.rs", "--grade-only"]);
        let Some(AstCmd::Tdg {
            grade_only,
            file_path,
        }) = cli.cmd
        else {
            panic!("expected Tdg");
        };
        assert!(grade_only);
        assert_eq!(file_path, "src/lib.rs");
    }

    #[test]
    fn meta_parses_depth_flag() {
        let cli = parse(&["ast", "meta", "src/lib.rs", "--depth", "summary"]);
        let Some(AstCmd::Meta { file_path, depth }) = cli.cmd else {
            panic!("expected Meta");
        };
        assert_eq!(file_path, "src/lib.rs");
        assert_eq!(depth, "summary");
    }

    #[test]
    fn meta_defaults_depth_to_skeleton() {
        let cli = parse(&["ast", "meta", "src/lib.rs"]);
        let Some(AstCmd::Meta { depth, .. }) = cli.cmd else {
            panic!("expected Meta");
        };
        assert_eq!(depth, "skeleton");
    }

    #[test]
    fn grep_parses_file_pattern_and_flags() {
        let cli = parse(&[
            "ast",
            "grep",
            "src/lib.rs",
            "$fn($$$args)",
            "--rewrite",
            "new_fn($$$args)",
            "--lang",
            "rust",
            "--top",
            "20",
        ]);
        let Some(AstCmd::Grep {
            file_path,
            pattern,
            rewrite,
            lang,
            top,
            skip_strings,
        }) = cli.cmd
        else {
            panic!("expected Grep");
        };
        assert_eq!(file_path, "src/lib.rs");
        assert_eq!(pattern, "$fn($$$args)");
        assert_eq!(rewrite.as_deref(), Some("new_fn($$$args)"));
        assert_eq!(lang.as_deref(), Some("rust"));
        assert_eq!(top, 20);
        assert!(!skip_strings);
    }

    #[test]
    fn grep_defaults_top_to_50() {
        let cli = parse(&["ast", "grep", "src/lib.rs", "$x"]);
        let Some(AstCmd::Grep { top, .. }) = cli.cmd else {
            panic!("expected Grep");
        };
        assert_eq!(top, 50);
    }

    #[test]
    fn format_rust_preserve_flag() {
        let cli = parse(&["ast", "format-rust", "--preserve", "src/lib.rs"]);
        let Some(AstCmd::FormatRust {
            preserve,
            file_path,
        }) = cli.cmd
        else {
            panic!("expected FormatRust");
        };
        assert!(preserve);
        assert_eq!(file_path, "src/lib.rs");
    }

    #[test]
    fn format_rust_no_preserve() {
        let cli = parse(&["ast", "format-rust", "src/lib.rs"]);
        let Some(AstCmd::FormatRust {
            preserve,
            file_path,
        }) = cli.cmd
        else {
            panic!("expected FormatRust");
        };
        assert!(!preserve);
        assert_eq!(file_path, "src/lib.rs");
    }

    #[test]
    fn workspace_info_defaults_dir_to_dot() {
        let cli = parse(&["ast", "workspace-info"]);
        let Some(AstCmd::WorkspaceInfo { dir }) = cli.cmd else {
            panic!("expected WorkspaceInfo");
        };
        assert_eq!(dir, ".");
    }

    #[test]
    fn node_types_defaults_lang_to_rust() {
        let cli = parse(&["ast", "node-types"]);
        let Some(AstCmd::NodeTypes { lang }) = cli.cmd else {
            panic!("expected NodeTypes");
        };
        assert_eq!(lang, "rust");
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(AstCli::try_parse_from(["ast", "frobnicate"]).is_err());
    }

    // ── render_blast_as_tree (preserved verbatim) ─────────────────────────────

    #[test]
    fn test_render_blast_as_tree_non_empty() {
        let json = r#"{"file_path":"src/lib.rs","blast_radius":3,"consumers":["a.rs","b.rs","c.rs"],"coedit_files":["d.rs"]}"#;
        let tree = render_blast_as_tree(json);
        assert!(tree.contains("src/lib.rs"), "tree: {tree}");
        assert!(tree.contains("blast_radius=3"), "tree: {tree}");
        assert!(tree.contains("Direct dependents"), "tree: {tree}");
        assert!(tree.contains("a.rs"), "tree: {tree}");
        assert!(tree.contains("Co-edit signals"), "tree: {tree}");
        assert!(tree.contains("d.rs"), "tree: {tree}");
    }

    #[test]
    fn test_render_blast_as_tree_empty_consumers() {
        let json =
            r#"{"file_path":"src/lib.rs","blast_radius":0,"consumers":[],"coedit_files":[]}"#;
        let tree = render_blast_as_tree(json);
        assert!(tree.contains("src/lib.rs"), "tree: {tree}");
        assert!(tree.contains("blast_radius=0"), "tree: {tree}");
        assert!(!tree.contains("Direct dependents"), "tree: {tree}");
        assert!(!tree.contains("Co-edit signals"), "tree: {tree}");
    }

    #[test]
    fn test_render_blast_as_tree_invalid_json() {
        let raw = "not valid json";
        let result = render_blast_as_tree(raw);
        assert_eq!(result, raw);
    }
}
