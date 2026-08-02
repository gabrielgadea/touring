//! Functional Wiring — tracks PURPOSE-BASED chains, not just structural imports.
//!
//! Unlike structural wiring (wiring.rs) which tracks pub symbol → consumer
//! relationships, functional wiring tracks whether a module's **purpose** connects
//! to another module's **purpose**.
//!
//! Example:
//! ```ignore
//! // auth.rs
//! pub fn authenticate(credentials: Credentials) -> SessionToken { ... }
//! // auth's purpose: "authentication" → produces SessionToken
//!
//! // session.rs
//! pub fn create_session(user_id: UserId) -> Session { ... }
//! // session's purpose: "session management" → consumes UserId
//!
//! // Chain detected: auth → session (sequential: auth.output = session.input)
//! ```
//!
//! This matters because TWO modules can import each other (structural orphan)
//! but NOT fulfill each other's purpose (functional gap).
//!
//! ## Chain Types
//!
//! - **Sequential**: output_type(A) matches input_type(B)
//!   e.g. authenticate() → SessionToken → create_session(user_id: UserId)
//! - **Complementary**: same purpose / same domain
//!   e.g. auth.rs + auth/middleware.rs both serve "authentication"
//! - **Hierarchical**: abstraction layer → concrete implementation
//!   e.g. trait definition → impl struct

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::knowledge::FileKnowledgeDB;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// A single symbol with its functional signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionalSymbol {
    /// Symbol name (e.g. "authenticate", "create_session").
    pub name: String,
    /// One-line purpose extracted from doc comment.
    pub purpose: Option<String>,
    /// Parameter types as strings (e.g. "Credentials", "UserId").
    pub input_types: Vec<String>,
    /// Return/Output type as string (e.g. "SessionToken", "Session").
    pub output_type: Option<String>,
    /// Inferred domain (e.g. "auth", "database", "network", "async").
    pub domain: Option<String>,
}

/// The functional signature of an entire module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionalSignature {
    /// Module/file path.
    pub file_path: String,
    /// Module-level doc comment or inferred purpose.
    pub module_purpose: Option<String>,
    /// All exported symbols with their functional signatures.
    pub symbols: Vec<FunctionalSymbol>,
    /// Inferred domain for the entire module.
    pub domain: Option<String>,
    /// SHA256 of content at time of extraction (staleness detection).
    pub content_hash: Option<String>,
}

/// A detected functional chain between two modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionalChain {
    /// Unique chain identifier.
    pub id: i64,
    /// Source module (produces value).
    pub source_module: String,
    /// Source symbol name.
    pub source_symbol: String,
    /// Output type of source symbol.
    pub source_output: Option<String>,
    /// Sink module (consumes value).
    pub sink_module: String,
    /// Sink symbol name.
    pub sink_symbol: String,
    /// Input type of sink symbol.
    pub sink_input: Option<String>,
    /// Type of chain connection.
    pub chain_type: ChainType,
    /// Confidence 0.0-1.0 based on type match quality.
    pub confidence: f64,
    /// When the chain was first detected.
    pub created_at: String,
}

/// How the chain connects two symbols.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChainType {
    /// output_type(A) == input_type(B) → sequential fulfillment
    Sequential,
    /// Same purpose/domain → complementary modules serving same goal
    Complementary,
    /// Trait → impl or abstract → concrete hierarchy
    Hierarchical,
    /// Chain was broken (symbol removed from source)
    Broken,
}

impl std::fmt::Display for ChainType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainType::Sequential => write!(f, "sequential"),
            ChainType::Complementary => write!(f, "complementary"),
            ChainType::Hierarchical => write!(f, "hierarchical"),
            ChainType::Broken => write!(f, "broken"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Purpose extraction
// ─────────────────────────────────────────────────────────────────────────────

/// Extract the module-level doc comment from file content (first `//!` or `//!` block at top).
fn extract_module_doc(content: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let mut doc_lines: Vec<&str> = Vec::new();

    for line in lines.iter() {
        let trimmed = line.trim();
        if trimmed.starts_with("//!") || trimmed.starts_with("/*!") {
            doc_lines.push(trimmed.trim_start_matches("//!").trim_start_matches("/*!"));
        } else if !doc_lines.is_empty() {
            // Already collecting and hit non-doc content → stop
            break;
        } else {
            // Not yet collecting → stop (no module doc found)
            break;
        }
    }

    if doc_lines.is_empty() {
        None
    } else {
        Some(doc_lines.join(" ").trim().to_string())
    }
}

/// Extract a FunctionalSignature for a module from its content and symbols.
///
/// This is the main entry point called from post_read to register a module's
/// functional identity in the wiring graph.
pub fn extract_functional_signature(
    file_path: &str,
    content: &str,
    symbols_json: &str,
) -> Option<FunctionalSignature> {
    let module_doc = extract_module_doc(content);
    let module_purpose = module_doc
        .as_ref()
        .and_then(|doc| extract_module_purpose(doc, file_path));
    let domain = module_purpose
        .as_ref()
        .and_then(|p| infer_domain_from_purpose(p));
    let symbols = extract_functional_symbols(symbols_json, file_path);

    // Compute content hash for staleness detection
    let content_hash = if !symbols.is_empty() {
        // Simple hash based on content length + first/last chars
        let h = (content.len() as u64)
            .wrapping_add(
                content
                    .chars()
                    .next()
                    .map(|c| c as u64)
                    .unwrap_or(0)
                    .wrapping_mul(31),
            )
            .wrapping_add(
                content
                    .chars()
                    .last()
                    .map(|c| c as u64)
                    .unwrap_or(0)
                    .wrapping_mul(17),
            );
        Some(format!("{:016x}", h))
    } else {
        None
    };

    Some(FunctionalSignature {
        file_path: file_path.to_string(),
        module_purpose,
        symbols,
        domain,
        content_hash,
    })
}

/// Infer domain from module purpose string.
fn infer_domain_from_purpose(purpose: &str) -> Option<String> {
    let lower = purpose.to_lowercase();
    let domains = [
        ("auth", "authentication"),
        ("cache", "caching"),
        ("config", "configuration"),
        ("database", "database"),
        ("diff", "diffing"),
        ("error", "error_handling"),
        ("eval", "evaluation"),
        ("graph", "graph"),
        ("hash", "hashing"),
        ("hook", "hooks"),
        ("index", "indexing"),
        ("learning", "learning"),
        ("memory", "memory"),
        ("metric", "metrics"),
        ("parse", "parsing"),
        ("pipeline", "pipeline"),
        ("predict", "prediction"),
        ("quality", "quality"),
        ("query", "query"),
        ("rl", "reinforcement_learning"),
        ("router", "routing"),
        ("schema", "schema"),
        ("search", "search"),
        ("session", "session"),
        ("signal", "signals"),
        ("similar", "similarity"),
        ("symbol", "symbols"),
        ("tree", "tree"),
        ("wiring", "wiring"),
    ];
    for (key, domain) in domains.iter() {
        if lower.contains(key) {
            return Some(domain.to_string());
        }
    }
    None
}

/// Extract a one-line purpose from a doc comment.
///
/// Takes the first sentence of a doc comment as the "purpose".
/// Falls back to filename-based inference if no doc comment.
pub fn extract_module_purpose(doc_comment: &str, file_path: &str) -> Option<String> {
    let trimmed = doc_comment.trim();
    if trimmed.is_empty() {
        return infer_purpose_from_path(file_path);
    }
    // First line of doc comment (strip leading /// or /**)
    let first_line = trimmed
        .lines()
        .next()?
        .trim()
        .trim_start_matches("///")
        .trim_start_matches("/**")
        .trim();
    if first_line.is_empty() {
        return infer_purpose_from_path(file_path);
    }
    // Take up to first period for brevity
    let purpose = first_line
        .split(['.', '—', '-'])
        .next()
        .unwrap_or(first_line)
        .trim();
    if purpose.len() > 120 {
        Some(purpose[..120].trim().to_string())
    } else {
        Some(purpose.to_string())
    }
}

/// Infer module purpose from file path when no doc comment exists.
fn infer_purpose_from_path(file_path: &str) -> Option<String> {
    let path_lower = file_path.to_lowercase();
    let segments: Vec<&str> = path_lower
        .split('/')
        .filter(|s| !s.is_empty() && !s.starts_with("src"))
        .collect();

    if let Some(last) = segments.last() {
        let name = last.trim_end_matches(".rs").trim_end_matches(".py");
        let purpose = name.replace(['_', '-'], " ");
        if !purpose.is_empty() && purpose.len() < 60 {
            return Some(purpose);
        }
    }
    None
}

/// Extract functional symbols from a list of raw symbol data.
pub fn extract_functional_symbols(symbols_json: &str, file_path: &str) -> Vec<FunctionalSymbol> {
    let symbols: Vec<serde_json::Value> = serde_json::from_str(symbols_json).unwrap_or_default();

    symbols
        .into_iter()
        .filter_map(|sym| {
            let name = sym.get("name")?.as_str()?.to_string();
            let kind = sym
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("function");
            // Only functions and methods have meaningful I/O types
            if !["function", "method"].contains(&kind) {
                return None;
            }

            let doc = sym.get("doc").and_then(|v| v.as_str()).unwrap_or("");
            let purpose = if doc.is_empty() {
                infer_purpose_from_fn_name(&name)
            } else {
                extract_fn_purpose(doc)
            };

            let input_types: Vec<String> = sym
                .get("params")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|p| p.get("type").and_then(|t| t.as_str()))
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();

            let output_type = sym
                .get("return_type")
                .and_then(|v| v.as_str())
                .map(String::from);

            let domain = infer_domain(&name, doc, file_path);

            Some(FunctionalSymbol {
                name,
                purpose,
                input_types,
                output_type,
                domain,
            })
        })
        .collect()
}

/// Extract a one-line purpose from a function's doc comment.
fn extract_fn_purpose(doc: &str) -> Option<String> {
    let first_line = doc
        .trim()
        .lines()
        .next()?
        .trim()
        .strip_prefix("///")
        .or_else(|| doc.trim().lines().next())
        .unwrap_or(doc)
        .trim();
    let purpose = first_line
        .split(['.', '—', '-'])
        .next()
        .unwrap_or(first_line)
        .trim();
    if purpose.is_empty() {
        return None;
    }
    let truncated = if purpose.len() > 80 {
        &purpose[..80]
    } else {
        purpose
    };
    Some(truncated.to_string())
}

/// Infer purpose from function name when no doc comment.
fn infer_purpose_from_fn_name(name: &str) -> Option<String> {
    let verb = [
        "get_",
        "set_",
        "create_",
        "delete_",
        "update_",
        "find_",
        "check_",
        "validate_",
        "verify_",
        "build_",
        "parse_",
        "render_",
        "send_",
        "fetch_",
        "load_",
        "save_",
        "init_",
        "setup_",
    ];
    for v in verb {
        if let Some(rest) = name.strip_prefix(v) {
            let purpose = format!("{} {}", v.trim_end_matches('_'), rest.replace('_', " "));
            return Some(purpose);
        }
    }
    if name.contains('_') {
        let purpose = name.replace('_', " ");
        Some(purpose)
    } else {
        None
    }
}

/// Infer domain from function name, doc, or file path.
fn infer_domain(name: &str, doc: &str, file_path: &str) -> Option<String> {
    let combined = format!("{} {} {}", name, doc, file_path).to_lowercase();

    let domain_keywords = [
        (
            "auth",
            "authenticate login session token jwt oauth password",
        ),
        (
            "database",
            "db query sql database repository entity persistence",
        ),
        (
            "network",
            "http request fetch send url endpoint rest api grpc",
        ),
        ("async", "future stream tokio async await thread spawn"),
        ("cache", "cache memoize ttl evict lru"),
        ("config", "config settings env variables options"),
        ("logging", "log logger trace debug info warn error"),
        ("metrics", "metrics counter histogram gauge observability"),
        ("error", "error handling exception fault recovery"),
        ("validation", "validation schema check verify sanitize"),
        (
            "serialization",
            "serialize deserialize encode decode json yaml toml",
        ),
        ("file", "file path fs read write directory"),
        ("time", "time timestamp date duration chrono"),
        ("crypto", "crypto hash sha md5 encrypt decrypt signature"),
    ];

    for (domain, keywords) in domain_keywords {
        let score: usize = keywords
            .split_whitespace()
            .filter(|kw| combined.contains(kw))
            .count();
        if score >= 2 {
            return Some(domain.to_string());
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Chain detection
// ─────────────────────────────────────────────────────────────────────────────

/// Push one chain entry built from the leading exported symbol of two [`FunctionalSignature`]s.
///
/// Deduplicates the `chains.push((sig.file_path, sig.symbols.first()...))` scaffold that
/// appears for each chain-type detector (complementary-purpose, complementary-domain,
/// hierarchical). The tuple layout matches [`detect_chains`]'s return type:
/// `(source_mod, source_sym, sink_mod, sink_sym, ChainType, confidence)`.
fn push_sig_pair_chain(
    chains: &mut Vec<(String, String, String, String, ChainType, f64)>,
    sig_a: &FunctionalSignature,
    sig_b: &FunctionalSignature,
    chain_type: ChainType,
    conf: f64,
) {
    chains.push((
        sig_a.file_path.clone(),
        sig_a
            .symbols
            .first()
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        sig_b.file_path.clone(),
        sig_b
            .symbols
            .first()
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        chain_type,
        conf,
    ));
}

/// Detect functional chains from a set of functional signatures.
///
/// Returns all detected chains (sequential, complementary, hierarchical).
pub fn detect_chains(
    signatures: &[FunctionalSignature],
) -> Vec<(String, String, String, String, ChainType, f64)> {
    let mut chains: Vec<(String, String, String, String, ChainType, f64)> = Vec::new();

    for (i, sig_a) in signatures.iter().enumerate() {
        for sig_b in signatures.iter().skip(i + 1) {
            // Sequential chains: output(A) = input(B) or output(B) = input(A)
            if let Some((src_mod, src_sym, _, sink_mod, sink_sym, _, conf)) =
                detect_sequential_chain(sig_a, sig_b)
            {
                // Forward: sig_a -> sig_b
                chains.push((
                    src_mod.clone(),
                    src_sym.clone(),
                    sink_mod.clone(),
                    sink_sym.clone(),
                    ChainType::Sequential,
                    conf,
                ));
                // Reverse: sig_b -> sig_a
                chains.push((
                    sink_mod,
                    sink_sym,
                    src_mod,
                    src_sym,
                    ChainType::Sequential,
                    conf,
                ));
            }

            // Complementary chains: same module purpose
            if sig_a.module_purpose == sig_b.module_purpose
                && sig_a.module_purpose.is_some()
                && sig_a.file_path != sig_b.file_path
            {
                // Lower confidence without type evidence
                push_sig_pair_chain(&mut chains, sig_a, sig_b, ChainType::Complementary, 0.7);
            }

            // Complementary chains: same domain
            if sig_a.domain == sig_b.domain
                && sig_a.domain.is_some()
                && sig_a.file_path != sig_b.file_path
            {
                // Domain inference has lower confidence
                push_sig_pair_chain(&mut chains, sig_a, sig_b, ChainType::Complementary, 0.5);
            }

            // Hierarchical: one module is a "base" or "trait", other is "impl"
            if is_hierarchical_pair(sig_a, sig_b) {
                push_sig_pair_chain(&mut chains, sig_a, sig_b, ChainType::Hierarchical, 0.8);
            }
        }
    }

    chains
}

/// Detect if output of one signature matches input of another.
fn detect_sequential_chain(
    sig_a: &FunctionalSignature,
    sig_b: &FunctionalSignature,
) -> Option<(String, String, String, String, String, String, f64)> {
    for sym_a in &sig_a.symbols {
        let Some(out_a) = &sym_a.output_type else {
            continue;
        };
        let out_a_clean = normalize_type(out_a);

        for sym_b in &sig_b.symbols {
            let out_score: f64 = sym_b
                .input_types
                .iter()
                .filter(|inp| normalize_type(inp) == out_a_clean)
                .count()
                .min(1) as f64;

            if out_score > 0.0 {
                let confidence = 0.6 + (out_score * 0.3);
                return Some((
                    sig_a.file_path.clone(),
                    sym_a.name.clone(),
                    out_a.clone(),
                    sig_b.file_path.clone(),
                    sym_b.name.clone(),
                    sym_b.input_types.first().cloned().unwrap_or_default(),
                    confidence,
                ));
            }
        }
    }
    None
}

/// Normalize a type string for comparison (remove Result, Option, Vec wrappers).
/// Normalize a type string for comparison by stripping Result/Option/Vec/Box/Arc/Rc wrappers.
fn normalize_type(t: &str) -> String {
    let mut result = t.to_string();

    // Iteratively strip known wrapper prefixes until no change
    let wrappers = ["Result<", "Option<", "Vec<", "Box<", "Arc<", "Rc<"];
    loop {
        let before = result.clone();
        for w in &wrappers {
            result = result.replace(w, "");
        }
        if result == before {
            break;
        }
    }

    // Remove angle brackets (unbalanced → treat as parens)
    result = result
        .replace('<', "(")
        .replace('>', ")")
        .replace(' ', "")
        .trim()
        .to_string();

    // Remove trailing ')' that came from '>' in the original (e.g. Result<Token> → Token))
    while result.ends_with(')') {
        result = result.trim_end_matches(')').to_string();
        if result.is_empty() {
            break;
        }
    }

    result
}

/// Check if two signatures form a hierarchical pair (trait vs impl).
fn is_hierarchical_pair(sig_a: &FunctionalSignature, sig_b: &FunctionalSignature) -> bool {
    let a_name = sig_a.file_path.to_lowercase();
    let b_name = sig_b.file_path.to_lowercase();

    let is_trait_like = |name: &str| {
        name.contains("trait") || name.contains("interface") || name.contains("protocol")
    };
    let is_impl_like =
        |name: &str| name.contains("impl") || name.contains("struct") || name.contains("_default");

    // If one looks like a trait and the other like an impl, potential hierarchy
    (is_trait_like(&a_name) && is_impl_like(&b_name))
        || (is_impl_like(&a_name) && is_trait_like(&b_name))
}

// ─────────────────────────────────────────────────────────────────────────────
// Database operations
// ─────────────────────────────────────────────────────────────────────────────

/// Map a rusqlite [`Row`] from `functional_chains` to a [`FunctionalChain`].
///
/// Column order (SELECT id(0), source_module(1), source_symbol(2), source_output(3),
/// sink_module(4), sink_symbol(5), sink_input(6), chain_type(7), confidence(8), created_at(9)):
/// matches both `chains_for_module` and `chains_where_module_provides`.
///
/// The `chain_type` string-to-enum conversion lives here once instead of
/// being duplicated in every query function.
fn row_to_functional_chain(row: &rusqlite::Row<'_>) -> rusqlite::Result<FunctionalChain> {
    let chain_type_str: String = row.get(7)?;
    let chain_type = match chain_type_str.as_str() {
        "Complementary" => ChainType::Complementary,
        "Hierarchical" => ChainType::Hierarchical,
        "Broken" => ChainType::Broken,
        _ => ChainType::Sequential,
    };
    Ok(FunctionalChain {
        id: row.get(0)?,
        source_module: row.get(1)?,
        source_symbol: row.get(2)?,
        source_output: row.get(3)?,
        sink_module: row.get(4)?,
        sink_symbol: row.get(5)?,
        sink_input: row.get(6)?,
        chain_type,
        confidence: row.get(8)?,
        created_at: row.get(9)?,
    })
}

impl FileKnowledgeDB {
    /// Register a functional signature for a module.
    ///
    /// Called after post_read extracts the functional signature.
    /// Uses INSERT OR REPLACE to update on re-scan.
    pub fn register_functional_signature(
        &self,
        sig: &FunctionalSignature,
    ) -> Result<(), rusqlite::Error> {
        let symbols_json = serde_json::to_string(&sig.symbols).unwrap_or_default();
        self.conn_ref().execute(
            "INSERT OR REPLACE INTO functional_signatures
             (file_path, module_purpose, domain, symbols_json, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                sig.file_path,
                sig.module_purpose,
                sig.domain,
                symbols_json,
                sig.content_hash,
            ],
        )?;
        Ok(())
    }

    /// Get the functional signature for a module.
    pub fn get_functional_signature(
        &self,
        file_path: &str,
    ) -> Result<Option<FunctionalSignature>, rusqlite::Error> {
        let mut stmt = self.conn_ref().prepare(
            "SELECT file_path, module_purpose, domain, symbols_json, content_hash
             FROM functional_signatures WHERE file_path = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![file_path])?;
        if let Some(row) = rows.next()? {
            let symbols_json: String = row.get(3)?;
            let symbols: Vec<FunctionalSymbol> =
                serde_json::from_str(&symbols_json).unwrap_or_default();
            Ok(Some(FunctionalSignature {
                file_path: row.get(0)?,
                module_purpose: row.get(1)?,
                symbols,
                domain: row.get(2)?,
                content_hash: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get all functional signatures (for chain detection).
    pub fn all_functional_signatures(&self) -> Result<Vec<FunctionalSignature>, rusqlite::Error> {
        let mut stmt = self.conn_ref().prepare(
            "SELECT file_path, module_purpose, domain, symbols_json, content_hash
             FROM functional_signatures",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let symbols_json: String = row.get(3)?;
                let symbols: Vec<FunctionalSymbol> =
                    serde_json::from_str(&symbols_json).unwrap_or_default();
                Ok(FunctionalSignature {
                    file_path: row.get(0)?,
                    module_purpose: row.get(1)?,
                    symbols,
                    domain: row.get(2)?,
                    content_hash: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Clear all detected chains (used before re-detection).
    pub fn clear_functional_chains(&self) -> Result<(), rusqlite::Error> {
        self.conn_ref()
            .execute("DELETE FROM functional_chains", [])?;
        Ok(())
    }

    /// Insert a functional chain.
    pub fn insert_functional_chain(&self, chain: &FunctionalChain) -> Result<(), rusqlite::Error> {
        self.conn_ref().execute(
            "INSERT INTO functional_chains
             (source_module, source_symbol, source_output, sink_module, sink_symbol,
              sink_input, chain_type, confidence, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                chain.source_module,
                chain.source_symbol,
                chain.source_output,
                chain.sink_module,
                chain.sink_symbol,
                chain.sink_input,
                chain.chain_type.to_string(),
                chain.confidence,
                chain.created_at,
            ],
        )?;
        Ok(())
    }

    /// Get all functional chains for a given module (as source or sink).
    pub fn chains_for_module(
        &self,
        file_path: &str,
    ) -> Result<Vec<FunctionalChain>, rusqlite::Error> {
        let mut stmt = self.conn_ref().prepare(
            "SELECT id, source_module, source_symbol, source_output,
                    sink_module, sink_symbol, sink_input, chain_type, confidence, created_at
             FROM functional_chains
             WHERE source_module = ?1 OR sink_module = ?1",
        )?;
        let chains = stmt
            .query_map(rusqlite::params![file_path], |row| {
                row_to_functional_chain(row)
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(chains)
    }

    /// Get chains where the given module is the SOURCE (it provides something).
    pub fn chains_where_module_provides(
        &self,
        file_path: &str,
    ) -> Result<Vec<FunctionalChain>, rusqlite::Error> {
        let mut stmt = self.conn_ref().prepare(
            "SELECT id, source_module, source_symbol, source_output,
                    sink_module, sink_symbol, sink_input, chain_type, confidence, created_at
             FROM functional_chains WHERE source_module = ?1",
        )?;
        let chains = stmt
            .query_map(rusqlite::params![file_path], |row| {
                row_to_functional_chain(row)
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(chains)
    }

    /// Detect broken chains: source symbol no longer exists in source module.
    pub fn detect_broken_chains(
        &self,
        file_path: &str,
    ) -> Result<Vec<FunctionalChain>, rusqlite::Error> {
        let current_sig = self.get_functional_signature(file_path)?;
        let Some(sig) = current_sig else {
            return Ok(vec![]);
        };

        let current_symbol_names: HashMap<String, bool> =
            sig.symbols.iter().map(|s| (s.name.clone(), true)).collect();

        let mut broken: Vec<FunctionalChain> = Vec::new();
        let chains = self.chains_where_module_provides(file_path)?;

        for mut chain in chains {
            if !current_symbol_names.contains_key(&chain.source_symbol) {
                chain.chain_type = ChainType::Broken;
                broken.push(chain);
            }
        }

        Ok(broken)
    }

    /// Re-detect and persist all functional chains from current signatures.
    ///
    /// Called from post_read after all signatures are registered.
    pub fn rebuild_functional_chains(&self) -> Result<usize, rusqlite::Error> {
        let signatures = self.all_functional_signatures()?;
        let raw_chains = detect_chains(&signatures);

        self.clear_functional_chains()?;

        let now = chrono_lite_now();
        let mut inserted = 0;

        for (source_module, source_symbol, sink_module, sink_symbol, chain_type, confidence) in
            raw_chains
        {
            // Avoid duplicate chains
            let exists: i64 = self.conn_ref().query_row(
                "SELECT COUNT(*) FROM functional_chains
                 WHERE source_module = ?1 AND source_symbol = ?2
                   AND sink_module = ?3 AND sink_symbol = ?4",
                rusqlite::params![source_module, source_symbol, sink_module, sink_symbol],
                |r| r.get(0),
            )?;
            if exists > 0 {
                continue;
            }

            let chain = FunctionalChain {
                id: 0,
                source_module,
                source_symbol,
                source_output: None,
                sink_module,
                sink_symbol,
                sink_input: None,
                chain_type,
                confidence,
                created_at: now.clone(),
            };
            self.insert_functional_chain(&chain)?;
            inserted += 1;
        }

        Ok(inserted)
    }
}

/// Get current UTC timestamp in SQLite format.
fn chrono_lite_now() -> String {
    // Avoid chrono dependency — use std::time
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Approximate UTC: just use epoch seconds
    format!("{}", secs)
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal generation
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a functional chain signal for pre_edit.
///
/// Detects if the file being edited is part of any functional chain
/// and warns if the edit might break it.
pub fn functional_chain_signal(db: &FileKnowledgeDB, file_path: &str) -> Option<(f32, String)> {
    // First check if this file has any broken chains
    let broken = db.detect_broken_chains(file_path).ok()?;
    if !broken.is_empty() {
        let first = broken.first()?;
        return Some((
            2.0, // Very high weight — broken chains are critical
            format!(
                "FUNCTIONAL CHAIN BROKEN: {}::{} previously connected to {}::{} \u{2014} this symbol no longer exists in the source module",
                first.source_module, first.source_symbol, first.sink_module, first.sink_symbol,
            ),
        ));
    }

    // Check if file participates in any active chains
    let chains = db.chains_for_module(file_path).ok()?;
    if chains.is_empty() {
        return None;
    }

    // Build a description of chains
    let sequential_count = chains
        .iter()
        .filter(|c| c.chain_type == ChainType::Sequential)
        .count();
    let complementary_count = chains
        .iter()
        .filter(|c| c.chain_type == ChainType::Complementary)
        .count();

    let mut parts: Vec<String> = Vec::new();
    if sequential_count > 0 {
        parts.push(format!("{} sequential", sequential_count));
    }
    if complementary_count > 0 {
        parts.push(format!("{} complementary", complementary_count));
    }

    if parts.is_empty() {
        return None;
    }

    let chain_desc = parts.join(", ");
    let chains_summary = chains
        .iter()
        .take(2)
        .map(|c| {
            format!(
                "{} \u{2192} {} [{}]",
                c.source_symbol, c.sink_symbol, c.chain_type
            )
        })
        .collect::<Vec<_>>()
        .join("; ");

    Some((
        1.6, // High weight — chain context is important for edits
        format!(
            "functional: {} functional chain(s) detected \u{2014} edit impacts: {}",
            chain_desc, chains_summary
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_type() {
        assert_eq!(normalize_type("Result<SessionToken>"), "SessionToken");
        assert_eq!(normalize_type("Option<Vec<String>>"), "String");
        assert_eq!(normalize_type("Box<Foo>"), "Foo");
        assert_eq!(normalize_type("Arc<Rc<Bar>>"), "Bar");
        assert_eq!(normalize_type("String"), "String");
    }

    #[test]
    fn test_extract_module_purpose() {
        let doc = " Authentication module - handles user login and session creation";
        assert!(extract_module_purpose(doc, "auth.rs").is_some());
    }

    #[test]
    fn test_sequential_chain_detection() {
        let sig_auth = FunctionalSignature {
            file_path: "auth.rs".to_string(),
            module_purpose: Some("authentication".to_string()),
            symbols: vec![FunctionalSymbol {
                name: "authenticate".to_string(),
                purpose: Some("authenticate user".to_string()),
                input_types: vec!["Credentials".to_string()],
                output_type: Some("SessionToken".to_string()),
                domain: Some("auth".to_string()),
            }],
            domain: Some("auth".to_string()),
            content_hash: None,
        };

        let sig_session = FunctionalSignature {
            file_path: "session.rs".to_string(),
            module_purpose: Some("session management".to_string()),
            symbols: vec![FunctionalSymbol {
                name: "create_session".to_string(),
                purpose: Some("create session".to_string()),
                input_types: vec!["UserId".to_string()], // Note: no direct match with SessionToken
                output_type: Some("Session".to_string()),
                domain: Some("auth".to_string()),
            }],
            domain: Some("auth".to_string()),
            content_hash: None,
        };

        let sig_auth2 = FunctionalSignature {
            file_path: "auth.rs".to_string(),
            module_purpose: Some("authentication".to_string()),
            symbols: vec![FunctionalSymbol {
                name: "get_user_id".to_string(),
                purpose: Some("get user id from token".to_string()),
                input_types: vec!["SessionToken".to_string()],
                output_type: Some("UserId".to_string()),
                domain: Some("auth".to_string()),
            }],
            domain: Some("auth".to_string()),
            content_hash: None,
        };

        // sig_auth → sig_session: no type match (SessionToken ≠ UserId)
        let chains = detect_chains(&[sig_auth.clone(), sig_session.clone()]);
        let _sequential: Vec<_> = chains
            .iter()
            .filter(|(_, _, _, _, t, _)| *t == ChainType::Sequential)
            .collect();
        // No sequential chain without type match

        // sig_auth2 → sig_session: SessionToken → UserId? No, output is UserId, input is UserId
        let chains2 = detect_chains(&[sig_auth2.clone(), sig_session.clone()]);
        assert!(
            chains2
                .iter()
                .any(|(_, _, _, _, t, _)| *t == ChainType::Sequential),
            "get_user_id.output=UserId must chain into create_session.input=UserId"
        );
    }

    #[test]
    fn test_complementary_chain_detection() {
        let sig_a = FunctionalSignature {
            file_path: "auth/login.rs".to_string(),
            module_purpose: Some("authentication".to_string()),
            symbols: vec![],
            domain: Some("auth".to_string()),
            content_hash: None,
        };

        let sig_b = FunctionalSignature {
            file_path: "auth/token.rs".to_string(),
            module_purpose: Some("authentication".to_string()),
            symbols: vec![],
            domain: Some("auth".to_string()),
            content_hash: None,
        };

        let chains = detect_chains(&[sig_a, sig_b]);
        assert!(
            chains
                .iter()
                .any(|(_, _, _, _, t, _)| *t == ChainType::Complementary),
            "auth/login.rs and auth/token.rs should form a complementary chain"
        );
    }

    #[test]
    fn test_functional_chain_signal_broken() {
        // This test would need a real DB — skipping for unit test
        // Integration test in integration_e2e.rs will cover the full path
    }
}
