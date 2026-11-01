//! Corpus mining — extract the *purpose prose* every artifact already carries.
//!
//! The portfolio's whole premise is that the signal exists but sits in a field
//! no index reads. This module reads that field, per format:
//!
//! | format | purpose source | fallback |
//! |---|---|---|
//! | `.py` | module docstring | `argparse(description=…)`, then bundle inheritance |
//! | `.rs` | leading `//!` header | — |
//! | `SKILL.md` | YAML frontmatter `description` | — |
//! | `*.toml` (ADW) | `[adw] description` | — |
//! | `.sh` | leading `#` comment block | — |
//!
//! Two grains are mined from the same read: the **artifact** (the table above)
//! and the **symbols** inside it — documented top-level `def`/`class` in Python
//! and `pub fn`/`struct`/`trait`/`enum` in Rust. The symbol grain answers "is
//! there already a function that does X?", which the name-keyed symbol index
//! cannot, because it matches identifiers rather than purpose. Bounded by
//! `MAX_SYMBOLS_PER_FILE` so a single module cannot flood a ranking.
//!
//! Bundle inheritance matters more than it sounds. Measured 2026-08-08: the
//! eight scripts of `~/.claude/skills/pdf-anthropic/scripts/` — the canonical
//! "generate a professional PDF" prior art — carry **no** module docstring,
//! while the bundle's `SKILL.md` describes them precisely. Mining scripts alone
//! would have missed the exact case this feature was asked for.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::{CapabilityEntry, CapabilityKind, Evidence};

/// Maximum bytes read per file. Purpose prose lives in the header; the tail is
/// only scanned for test markers, which appear well inside this window.
const MAX_READ_BYTES: usize = 256 * 1024;

/// Minimum useful length of mined prose — shorter than this is a label, not a purpose.
const MIN_PURPOSE_LEN: usize = 20;

/// Purpose prose is truncated to this many chars in the index (keeps it ~2 MB).
const MAX_PURPOSE_LEN: usize = 600;

/// Directory names never worth mining.
const SKIP_DIRS: &[&str] = &[
    ".venv",
    "venv",
    "site-packages",
    "node_modules",
    "__pycache__",
    ".git",
    "target",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "dist",
    "build",
    ".tox",
    ".cache",
];

/// Collapse `$HOME` to `~` so records are portable and readable.
#[must_use]
pub fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    home::home_dir()
        .map(|h| h.to_string_lossy().to_string())
        .filter(|h| s.starts_with(h.as_str()))
        .map_or_else(|| s.clone(), |h| format!("~{}", &s[h.len()..]))
}

/// The roots mined by default: skills bundles, every project's `scripts/`, the
/// ADW library, and the current workspace's Rust module headers.
#[must_use]
pub fn default_roots() -> Vec<PathBuf> {
    let Some(home) = home::home_dir() else {
        return Vec::new();
    };
    let mut roots = vec![home.join(".claude/skills")];
    if let Ok(entries) = std::fs::read_dir(home.join("projects")) {
        for e in entries.flatten() {
            let scripts = e.path().join("scripts");
            if scripts.is_dir() {
                roots.push(scripts);
            }
        }
    }
    roots.retain(|p| p.exists());
    roots
}

/// Read at most [`MAX_READ_BYTES`] of a file as lossy UTF-8.
fn read_head(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let mut cut = bytes.len().min(MAX_READ_BYTES);
    // Back off to a UTF-8 boundary so a truncated read never splits a codepoint
    // into replacement characters. `0b10xxxxxx` is a continuation byte.
    while cut > 0 && cut < bytes.len() && (bytes[cut] & 0xC0) == 0x80 {
        cut -= 1;
    }
    Some(String::from_utf8_lossy(&bytes[..cut]).into_owned())
}

/// Normalize mined prose: collapse whitespace, strip decoration, cap length.
fn clean_prose(raw: &str) -> Option<String> {
    let collapsed = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| c == '-' || c == '=' || c == '#' || c.is_whitespace())
        .to_string();
    if collapsed.len() < MIN_PURPOSE_LEN {
        return None;
    }
    Some(collapsed.chars().take(MAX_PURPOSE_LEN).collect())
}

/// Extract the leading module docstring of a Python source.
///
/// Skips shebang, encoding comments, `from __future__` imports and blank lines,
/// then captures a leading triple- or single-quoted string literal.
#[must_use]
pub fn python_docstring(src: &str) -> Option<String> {
    let mut rest = src;
    loop {
        let line = rest.lines().next()?;
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') || t.starts_with("from __future__") {
            let advance = line.len() + usize::from(rest.len() > line.len());
            rest = rest.get(advance..)?;
            continue;
        }
        break;
    }
    let t = rest.trim_start();
    for delim in ["\"\"\"", "'''"] {
        if let Some(body) = t.strip_prefix(delim)
            && let Some(end) = body.find(delim)
        {
            return clean_prose(&body[..end]);
        }
    }
    // Single-line quoted docstring.
    for delim in ['"', '\''] {
        if let Some(body) = t.strip_prefix(delim)
            && let Some(end) = body.find(delim)
        {
            return clean_prose(&body[..end]);
        }
    }
    None
}

/// Extract the string assigned to an `argparse` `description=` keyword.
#[must_use]
pub fn argparse_description(src: &str) -> Option<String> {
    let idx = src
        .find("description=")
        .or_else(|| src.find("description ="))?;
    let after = &src[idx..];
    let eq = after.find('=')?;
    let value = after.get(eq + 1..)?.trim_start();
    for delim in ["\"\"\"", "'''"] {
        if let Some(body) = value.strip_prefix(delim)
            && let Some(end) = body.find(delim)
        {
            return clean_prose(&body[..end]);
        }
    }
    for delim in ['"', '\''] {
        if let Some(body) = value.strip_prefix(delim)
            && let Some(end) = body.find(delim)
        {
            return clean_prose(&body[..end]);
        }
    }
    None
}

/// Extract the leading `//!` module header of a Rust source.
#[must_use]
pub fn rust_module_doc(src: &str) -> Option<String> {
    let mut out = String::new();
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("#!") || t.starts_with("#![") || t.is_empty() && out.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix("//!") {
            out.push_str(rest.trim());
            out.push(' ');
            continue;
        }
        if !out.is_empty() {
            break;
        }
        if t.starts_with("//") {
            continue;
        }
        break;
    }
    clean_prose(&out)
}

/// Extract `name` and `description` from a Markdown YAML frontmatter block.
#[must_use]
pub fn markdown_frontmatter(src: &str) -> Option<(Option<String>, String)> {
    let body = src.strip_prefix("---")?;
    let end = body.find("\n---")?;
    let block = &body[..end];
    let mut name = None;
    let mut desc = String::new();
    let mut in_desc = false;
    for line in block.lines() {
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(v.trim().to_string());
            in_desc = false;
        } else if let Some(v) = line.strip_prefix("description:") {
            in_desc = true;
            let v = v.trim();
            if v != ">" && v != "|" && !v.is_empty() {
                desc.push_str(v.trim_matches('"'));
                desc.push(' ');
            }
        } else if in_desc && (line.starts_with("  ") || line.starts_with('\t')) {
            desc.push_str(line.trim());
            desc.push(' ');
        } else if !line.trim().is_empty() {
            in_desc = false;
        }
    }
    clean_prose(&desc).map(|d| (name, d))
}

/// Strip `{{vars.x}}` / `{{nodes.y.summary}}` placeholders from indexable prose.
///
/// A template marker is machinery, not meaning: left in, every flow shares the
/// tokens `vars` and `summary`, which is noise every one of them matches on.
fn strip_templates(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        out.push(' ');
        match rest[start..].find("}}") {
            Some(end) => rest = &rest[start + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Append the prose of a `[purpose]` block, including when NOT to use the flow.
///
/// The negative case is indexed deliberately (E4 honesty): a portfolio entry that
/// only advertises cannot be ruled out by the next author reading it.
fn push_purpose(doc: &mut String, purpose: &toml::Value) {
    for key in [
        "intent",
        "when_to_use",
        "when_not_to_use",
        "inputs",
        "produces",
        "tags",
    ] {
        match purpose.get(key) {
            Some(toml::Value::String(s)) => {
                doc.push_str(s);
                doc.push(' ');
            }
            Some(toml::Value::Array(items)) => {
                for item in items.iter().filter_map(toml::Value::as_str) {
                    doc.push_str(item);
                    doc.push(' ');
                }
            }
            _ => {}
        }
    }
}

/// Append what the flow actually DOES — its steps, their kinds, the prose its
/// agents are given, the postures its critics hold, and the fragments it is
/// built from. A flow's purpose lives in its steps at least as much as in its
/// one-line description.
fn push_steps(doc: &mut String, parsed: &toml::Value) {
    if let Some(nodes) = parsed.get("node").and_then(toml::Value::as_table) {
        for (name, body) in nodes {
            doc.push_str(name);
            doc.push(' ');
            for key in ["type", "prompt", "message"] {
                if let Some(s) = body.get(key).and_then(toml::Value::as_str) {
                    doc.push_str(&strip_templates(s));
                    doc.push(' ');
                }
            }
            if let Some(persona) = body.get("persona") {
                for key in ["role", "stance", "lens", "bar", "scope"] {
                    if let Some(s) = persona.get(key).and_then(toml::Value::as_str) {
                        doc.push_str(&strip_templates(s));
                        doc.push(' ');
                    }
                }
            }
        }
    }
    if let Some(uses) = parsed.get("use").and_then(toml::Value::as_array) {
        for module in uses
            .iter()
            .filter_map(|u| u.get("module").and_then(toml::Value::as_str))
        {
            doc.push_str(module);
            doc.push(' ');
        }
    }
}

/// Extract the indexable purpose document of an ADW spec.
///
/// A flow is found by INTENT, and until 2026-08-18 this returned `[adw]
/// description` alone — one sentence, roughly ten words. Shell scripts enter the
/// same corpus carrying their whole header comment (50-300 words), and BM25
/// length normalisation does the rest: every ADW was unreachable. Reproduced with
/// the query that is *literally* `bugfix.toml`'s description — it returned a
/// Python script from an unrelated project and not one flow.
///
/// The spec already holds the rest of the answer: its header comment, its
/// `[purpose]` block, and the steps themselves. The document is composed from all
/// of it, so a flow is retrievable by what it is FOR rather than by its filename.
#[must_use]
pub fn adw_description(src: &str) -> Option<(Option<String>, String)> {
    let parsed: toml::Value = src.parse().ok()?;
    // The `[adw]` table is what makes this an ADW spec rather than any other TOML
    // in the tree (Cargo.toml, pyproject.toml, …), so it stays required.
    let adw = parsed.get("adw")?;
    let name = adw.get("name").and_then(|n| n.as_str()).map(str::to_string);

    let mut doc = String::new();
    // Order is a budget decision, not a formatting one. The document is capped at
    // MAX_PURPOSE_LEN, and an ADW header is mostly boilerplate — "Instantiate:",
    // "Run: --var ...". Leading with it spent the cap on invocation syntax and cut
    // the curated block mid-sentence: `when_not_to_use`, the field that lets the
    // portfolio rule a flow OUT, never reached the corpus at all. So when a flow
    // states its purpose, that goes first and the header takes what is left.
    let purpose = parsed.get("purpose");
    if let Some(purpose) = purpose {
        push_purpose(&mut doc, purpose);
    }
    if let Some(desc) = adw.get("description").and_then(toml::Value::as_str) {
        doc.push_str(desc);
        doc.push(' ');
    }
    push_steps(&mut doc, &parsed);
    if let Some(header) = shell_header(src) {
        doc.push_str(&header);
        doc.push(' ');
    }
    clean_prose(&doc).map(|d| (name, d))
}

/// Extract the leading `#` comment block of a shell script (after the shebang).
#[must_use]
pub fn shell_header(src: &str) -> Option<String> {
    let mut out = String::new();
    for (i, line) in src.lines().enumerate() {
        let t = line.trim_start();
        if i == 0 && t.starts_with("#!") {
            continue;
        }
        if let Some(rest) = t.strip_prefix('#') {
            out.push_str(rest.trim());
            out.push(' ');
            continue;
        }
        if t.is_empty() && out.is_empty() {
            continue;
        }
        break;
    }
    clean_prose(&out)
}

/// Minimum prose length for a SYMBOL to enter the index.
///
/// Higher than [`MIN_PURPOSE_LEN`] on purpose. Measured 2026-08-08: with the
/// module floor (20), stub docstrings like "Command-line interface." (23 chars)
/// entered the corpus and outranked real artifacts — BM25 length normalization
/// favours short documents, so a generic one-liner beats a precise paragraph.
/// A symbol earns a slot only when someone bothered to describe it.
const MIN_SYMBOL_PURPOSE_LEN: usize = 40;

/// Cap on documented symbols indexed per file.
///
/// Bounded on purpose: a portfolio that returns forty functions from one module
/// is noise, and the corpus would grow an order of magnitude. The head of a
/// file's documented API is where its representative capabilities live.
const MAX_SYMBOLS_PER_FILE: usize = 12;

/// One documented function/class/struct/trait found inside a file.
pub struct MinedSymbol {
    /// The declared name.
    pub name: String,
    /// The prose documenting it.
    pub purpose: String,
}

/// Extract documented top-level `def`/`class` declarations from Python source.
///
/// Only declarations whose docstring clears `MIN_PURPOSE_LEN` are kept: an
/// undocumented or one-word-documented function has no purpose to index.
#[must_use]
pub fn python_symbols(src: &str) -> Vec<MinedSymbol> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        // Top level only — indented defs are methods/closures, usually detail.
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let name = line
            .strip_prefix("def ")
            .or_else(|| line.strip_prefix("async def "))
            .or_else(|| line.strip_prefix("class "))
            .and_then(|rest| rest.split(['(', ':', ' ']).next())
            .filter(|n| !n.is_empty() && !n.starts_with('_'));
        let Some(name) = name else { continue };
        // The docstring opens on one of the next few lines (after the signature,
        // which may wrap across lines).
        let body: String = lines
            .iter()
            .skip(i + 1)
            .take(8)
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(purpose) = python_docstring(&body) {
            out.push(MinedSymbol {
                name: name.to_string(),
                purpose,
            });
            if out.len() >= MAX_SYMBOLS_PER_FILE {
                break;
            }
        }
    }
    out
}

/// Extract documented `pub` items from Rust source (`///` doc comments).
#[must_use]
pub fn rust_symbols(src: &str) -> Vec<MinedSymbol> {
    let mut out = Vec::new();
    let mut doc = String::new();
    for line in src.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("///") {
            doc.push_str(rest.trim());
            doc.push(' ');
            continue;
        }
        if t.starts_with("#[") || t.is_empty() {
            // Attributes and blank lines sit between the doc and the item.
            continue;
        }
        let name = t
            .strip_prefix("pub ")
            .map(|r| r.trim_start_matches("async ").trim_start_matches("unsafe "))
            .and_then(|r| {
                ["fn ", "struct ", "trait ", "enum ", "type "]
                    .iter()
                    .find_map(|kw| r.strip_prefix(kw))
            })
            .and_then(|rest| rest.split(['(', '<', ' ', '{', ';', ':']).next())
            .filter(|n| !n.is_empty());
        if let Some(name) = name
            && let Some(purpose) = clean_prose(&doc)
        {
            out.push(MinedSymbol {
                name: name.to_string(),
                purpose,
            });
            if out.len() >= MAX_SYMBOLS_PER_FILE {
                break;
            }
        }
        doc.clear();
    }
    out
}

/// Documented symbols inside a file, by language.
fn mine_symbols(path: &Path, content: &str) -> Vec<MinedSymbol> {
    let mut syms = match path.extension().and_then(|e| e.to_str()) {
        Some("py") => python_symbols(content),
        Some("rs") => rust_symbols(content),
        _ => return Vec::new(),
    };
    syms.retain(|s| s.purpose.len() >= MIN_SYMBOL_PURPOSE_LEN);
    syms
}

/// Heuristic: does this artifact have a test we can point at?
fn detect_tests(path: &Path, content: &str) -> Option<bool> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => Some(content.contains("#[cfg(test)]")),
        Some("py") => {
            let stem = path.file_stem()?.to_string_lossy().to_string();
            let dir = path.parent()?;
            let siblings = [
                dir.join(format!("test_{stem}.py")),
                dir.join(format!("{stem}_test.py")),
                dir.join("tests").join(format!("test_{stem}.py")),
            ];
            Some(siblings.iter().any(|p| p.exists()) || content.contains("def test_"))
        }
        _ => None,
    }
}

/// Age of a file in whole days, when the filesystem reports a mtime.
fn age_days(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let secs = std::time::SystemTime::now()
        .duration_since(modified)
        .ok()?
        .as_secs();
    Some(secs / 86_400)
}

/// Provenance label: the skill bundle or project a path belongs to.
fn provenance_of(path: &Path) -> String {
    let disp = display_path(path);
    if let Some(rest) = disp.strip_prefix("~/.claude/skills/") {
        return format!("skill:{}", rest.split('/').next().unwrap_or("?"));
    }
    if let Some(rest) = disp.strip_prefix("~/projects/") {
        return format!("project:{}", rest.split('/').next().unwrap_or("?"));
    }
    "workspace".to_string()
}

/// Should this directory entry be skipped entirely?
fn is_skipped(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|s| SKIP_DIRS.contains(&s))
    })
}

/// Collect every candidate file under `roots`.
fn collect_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        let walker = ignore::WalkBuilder::new(root)
            .standard_filters(false)
            .hidden(false)
            .follow_links(false)
            .build();
        for entry in walker.flatten() {
            let path = entry.path();
            if !entry.file_type().is_some_and(|t| t.is_file()) || is_skipped(path) {
                continue;
            }
            let keep = match path.extension().and_then(|e| e.to_str()) {
                Some("py" | "rs" | "sh" | "toml") => true,
                Some("md") => path.file_name().is_some_and(|n| n == "SKILL.md"),
                _ => false,
            };
            if keep {
                out.push(path.to_path_buf());
            }
        }
    }
    out
}

/// Purpose prose plus the metadata that depends on the format.
struct Mined {
    purpose: String,
    kind: CapabilityKind,
    language: &'static str,
    name: Option<String>,
    entry_point: Option<String>,
}

/// Mine one file, or `None` when it carries no usable purpose prose.
fn mine_file(path: &Path, content: &str) -> Option<Mined> {
    let disp = display_path(path);
    match path.extension().and_then(|e| e.to_str()) {
        Some("py") => {
            let purpose = python_docstring(content).or_else(|| argparse_description(content))?;
            Some(Mined {
                purpose,
                kind: CapabilityKind::Script,
                language: "python",
                name: None,
                entry_point: Some(format!("python3 {disp}")),
            })
        }
        Some("rs") => Some(Mined {
            purpose: rust_module_doc(content)?,
            kind: CapabilityKind::Module,
            language: "rust",
            name: None,
            entry_point: None,
        }),
        Some("sh") => Some(Mined {
            purpose: shell_header(content)?,
            kind: CapabilityKind::Script,
            language: "shell",
            name: None,
            entry_point: Some(format!("bash {disp}")),
        }),
        Some("md") => {
            let (name, purpose) = markdown_frontmatter(content)?;
            Some(Mined {
                purpose,
                kind: CapabilityKind::Skill,
                language: "markdown",
                name,
                entry_point: None,
            })
        }
        Some("toml") => {
            let (name, purpose) = adw_description(content)?;
            let invoke = name
                .as_deref()
                .map(|n| format!("touring adw run {n}"))
                .unwrap_or_else(|| "touring adw run <name>".to_string());
            Some(Mined {
                purpose,
                kind: CapabilityKind::Adw,
                language: "toml",
                name,
                entry_point: Some(invoke),
            })
        }
        _ => None,
    }
}

/// Keywords worth a field boost: the file stem split on separators, plus the
/// provenance bundle name.
fn keywords_for(path: &Path, provenance: &str) -> Vec<String> {
    let mut kws: Vec<String> = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .into_iter()
        .flat_map(|stem| {
            stem.split(['_', '-', '.'])
                .filter(|p| p.len() >= 2)
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        })
        .collect();
    if let Some((_, bundle)) = provenance.split_once(':') {
        kws.extend(
            bundle
                .split(['-', '_'])
                .filter(|p| p.len() >= 2)
                .map(str::to_lowercase),
        );
    }
    kws.sort();
    kws.dedup();
    kws
}

/// Evidence common to every entry mined from `path`.
fn evidence_for(path: &Path, content: &str) -> Evidence {
    Evidence {
        has_tests: detect_tests(path, content),
        modified_days_ago: age_days(path),
        prior_verdict: None,
        reward: None,
    }
}

/// Build the artifact-level entry, falling back to bundle inheritance.
///
/// Returns `None` when the file has neither its own purpose prose nor an
/// enclosing bundle that describes it.
fn build_file_entry(
    path: &Path,
    content: &str,
    mined: Option<&Mined>,
    inherited: Option<String>,
) -> Option<CapabilityEntry> {
    let disp = display_path(path);
    let provenance = provenance_of(path);
    let (purpose, kind, language, name, entry_point, purpose_inherited) = match mined {
        Some(m) => (
            m.purpose.clone(),
            m.kind,
            m.language,
            m.name.clone(),
            m.entry_point.clone(),
            false,
        ),
        None => {
            // Bundle inheritance rescues prose-less scripts (the pdf-anthropic case).
            let ext = path.extension().and_then(|e| e.to_str())?;
            let runner = match ext {
                "py" => "python3",
                "sh" => "bash",
                _ => return None,
            };
            (
                inherited?,
                CapabilityKind::Script,
                if ext == "py" { "python" } else { "shell" },
                None,
                Some(format!("{runner} {disp}")),
                true,
            )
        }
    };
    let name = name.unwrap_or_else(|| {
        path.file_stem()
            .map_or_else(|| disp.clone(), |s| s.to_string_lossy().to_string())
    });
    Some(CapabilityEntry {
        id: CapabilityEntry::make_id(kind, &disp),
        display_path: disp,
        kind,
        name,
        purpose,
        language: language.to_string(),
        entry_point,
        keywords: keywords_for(path, &provenance),
        provenance,
        evidence: evidence_for(path, content),
        purpose_inherited,
    })
}

/// Build the symbol-level entries for one file (the finer grain).
fn build_symbol_entries(path: &Path, content: &str) -> Vec<CapabilityEntry> {
    let language = match path.extension().and_then(|e| e.to_str()) {
        Some("py") => "python",
        Some("rs") => "rust",
        _ => return Vec::new(),
    };
    let disp = display_path(path);
    let provenance = provenance_of(path);
    let file_keywords = keywords_for(path, &provenance);
    let evidence = evidence_for(path, content);
    mine_symbols(path, content)
        .into_iter()
        .map(|sym| {
            let mut keywords = file_keywords.clone();
            keywords.extend(
                sym.name
                    .split(['_', '-'])
                    .filter(|p| p.len() >= 2)
                    .map(str::to_lowercase),
            );
            keywords.sort();
            keywords.dedup();
            CapabilityEntry {
                id: CapabilityEntry::make_symbol_id(&disp, &sym.name),
                display_path: disp.clone(),
                kind: CapabilityKind::Symbol,
                name: sym.name,
                purpose: sym.purpose,
                language: language.to_string(),
                entry_point: None,
                keywords,
                provenance: provenance.clone(),
                evidence: evidence.clone(),
                purpose_inherited: false,
            }
        })
        .collect()
}

/// Map every skill-bundle directory to the description in its `SKILL.md`.
fn bundle_index(mined: &[(PathBuf, String, Option<Mined>)]) -> HashMap<PathBuf, String> {
    let mut map = HashMap::new();
    for (path, _, m) in mined {
        if let Some(m) = m
            && m.kind == CapabilityKind::Skill
            && let Some(dir) = path.parent()
        {
            map.insert(dir.to_path_buf(), m.purpose.clone());
        }
    }
    map
}

/// Nearest enclosing bundle description for `path`, if any.
fn inherited_purpose(bundles: &HashMap<PathBuf, String>, path: &Path) -> Option<String> {
    let mut cur = path.parent();
    while let Some(dir) = cur {
        if let Some(p) = bundles.get(dir) {
            return Some(p.clone());
        }
        cur = dir.parent();
    }
    None
}

/// Mine every root into capability records, at two grains.
///
/// Three passes: read+classify each file, index the skill bundles so prose-less
/// scripts can inherit, then emit the artifact-level and symbol-level entries.
/// Parallelized with rayon — the walk is IO-bound, the parsing CPU-bound.
#[must_use]
pub fn mine(roots: &[PathBuf]) -> Vec<CapabilityEntry> {
    let files = collect_files(roots);

    let mined: Vec<(PathBuf, String, Option<Mined>)> = files
        .par_iter()
        .filter_map(|path| {
            let content = read_head(path)?;
            let m = mine_file(path, &content);
            Some((path.clone(), content, m))
        })
        .collect();

    let bundles = bundle_index(&mined);

    let mut entries: Vec<CapabilityEntry> = mined
        .par_iter()
        .flat_map(|(path, content, m)| {
            let mut out = build_symbol_entries(path, content);
            let inherited = if m.is_none() {
                inherited_purpose(&bundles, path)
            } else {
                None
            };
            if let Some(file_entry) = build_file_entry(path, content, m.as_ref(), inherited) {
                out.push(file_entry);
            }
            out
        })
        .collect();

    // Deterministic order (REGRA #17): identity never depends on walk order.
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries.dedup_by(|a, b| a.id == b.id);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mines_python_module_docstring_past_shebang_and_comments() {
        let src = "#!/usr/bin/env python3\n# -*- coding: utf-8 -*-\n\n\"\"\"render_map — draw the module dependency map as SVG.\"\"\"\nimport sys\n";
        let got = python_docstring(src).expect("docstring");
        assert!(got.contains("dependency map"), "{got}");
    }

    #[test]
    fn falls_back_to_argparse_description() {
        let src = "import argparse\np = argparse.ArgumentParser(description=\"Generate a professional PDF from HTML templates\")\n";
        assert!(python_docstring(src).is_none());
        let got = argparse_description(src).expect("argparse description");
        assert!(got.contains("professional PDF"), "{got}");
    }

    #[test]
    fn short_prose_is_rejected_as_a_label() {
        // "main" is a label, not a purpose — indexing it would be noise.
        assert!(python_docstring("\"\"\"main\"\"\"\n").is_none());
        assert!(clean_prose("todo").is_none());
    }

    #[test]
    fn mines_rust_module_header_skipping_inner_attributes() {
        let src = "#![allow(clippy::all)]\n//! SIMD-accelerated similarity search for the blast radius engine.\n//! Second line.\n\nuse std::fmt;\n";
        let got = rust_module_doc(src).expect("module doc");
        assert!(got.starts_with("SIMD-accelerated"), "{got}");
        assert!(got.contains("Second line"), "{got}");
    }

    #[test]
    fn mines_skill_frontmatter_including_folded_descriptions() {
        let src = "---\nname: pdf\ndescription: Comprehensive PDF manipulation toolkit for extracting text and creating documents.\n---\n# body\n";
        let (name, desc) = markdown_frontmatter(src).expect("frontmatter");
        assert_eq!(name.as_deref(), Some("pdf"));
        assert!(desc.contains("PDF manipulation"), "{desc}");

        let folded = "---\nname: x\ndescription: >\n  Generate professional reports\n  from structured data.\n---\n";
        let (_, d2) = markdown_frontmatter(folded).expect("folded frontmatter");
        assert!(
            d2.contains("professional reports") && d2.contains("structured data"),
            "{d2}"
        );
    }

    #[test]
    fn mines_adw_description_and_derives_invocation() {
        let src = "[adw]\nname = \"feature\"\ndescription = \"Full feature pipeline: expert memory, SOTA scout, plan, verified build\"\n";
        let (name, desc) = adw_description(src).expect("adw");
        assert_eq!(name.as_deref(), Some("feature"));
        assert!(desc.contains("feature pipeline"), "{desc}");
        // A non-ADW toml must not be mined as one.
        assert!(adw_description("[package]\nname = \"x\"\n").is_none());
    }

    /// The whole point of C1: a flow must be findable by the words that describe
    /// its PURPOSE, not just the handful in its one-line description.
    #[test]
    fn adw_document_composes_header_purpose_and_steps() {
        let src = r#"# ADW `bugfix` — memory-pack recall feeds a scoped fixer; a gate verifies.
[adw]
name = "bugfix"
description = "Expert bugfix: institutional recall feeds a workhorse fixer"

[purpose]
intent = "corrigir um bug com memoria institucional e gate de verificacao"
when_to_use = ["a reproducible defect in an indexed codebase"]
when_not_to_use = ["a production incident — use hotfix instead"]
produces = ["a verified minimal fix"]

[[use]]
module = "recall-pack"
as = "recall"

[node.fix]
type = "agent"
prompt = "Root-cause it; smallest correct fix. Memory: {{nodes.recall.summary}}"

[node.fix.persona]
role = "fixer"
lens = "root cause over symptom"
"#;
        let (name, doc) = adw_description(src).expect("adw");
        assert_eq!(name.as_deref(), Some("bugfix"));
        for needle in [
            "memory-pack recall",      // the header comment
            "institutional recall",    // [adw] description
            "memoria institucional",   // [purpose] intent
            "production incident",     // when_not_to_use — the honest negative
            "recall-pack",             // the fragment it composes
            "smallest correct fix",    // the step's own prose
            "root cause over symptom", // the persona's lens
        ] {
            assert!(doc.contains(needle), "missing {needle:?} in {doc}");
        }
        assert!(
            !doc.contains("{{") && !doc.contains("nodes.recall"),
            "template machinery must not enter the corpus: {doc}"
        );
    }

    /// The cap must fall on boilerplate, never on the curated block.
    ///
    /// `adw_document_composes_header_purpose_and_steps` proved the fields are read,
    /// but with a one-line header — so it never met MAX_PURPOSE_LEN. Every shipped
    /// flow carries a real header ("Instantiate: …", "Run: --var …"), and leading
    /// with it spent the whole budget on invocation syntax: the document ended
    /// mid-sentence inside `when_to_use`, and `when_not_to_use` — the one field the
    /// portfolio needs to rule a flow OUT — never entered the corpus at all.
    #[test]
    fn a_long_header_never_crowds_out_the_field_that_rules_a_flow_out() {
        let src = r#"# ADW `scout-perpetuo` (F1.7) — exploration as a permanent background process.
# Instantiate: touring adw from-template scout-perpetuo
# Run:         touring adw run scout-perpetuo --var topic="<what to keep scouting>"
#              (pauses at act_or_wait; resume with --approve act_or_wait to act now —
#               approving does NOT stop the scout: re-run cycles any time; cadence
#               state lives in .touring/scout-perpetuo/<slug>.json)
[adw]
name = "scout-perpetuo"
description = "Perpetual scout: yield-adaptive explore cycles then tickets then an act-vs-wait gate"

[purpose]
intent = "Keep scouting one topic on a yield-adaptive cadence, turning findings into tickets"
when_to_use = ["you want a standing demand generator instead of exploration as a one-off phase"]
when_not_to_use = ["a single question you want answered now — use explore-plan instead"]

[node.cycle]
type = "code"
"#;
        let (_, doc) = adw_description(src).expect("adw");
        assert!(
            doc.contains("use explore-plan instead"),
            "when_not_to_use was truncated away by the header ({} chars): {doc}",
            doc.len()
        );
    }

    /// Regression for the invisibility this replaced: the composed document has to
    /// be substantially longer than the one sentence that used to represent a flow,
    /// or BM25 length normalisation keeps burying it under shell scripts.
    #[test]
    fn adw_document_is_richer_than_the_bare_description() {
        let bare = "[adw]\nname = \"x\"\ndescription = \"Expert bugfix: institutional recall feeds a workhorse fixer\"\n";
        let rich = format!(
            "{bare}\n[purpose]\nintent = \"corrigir um bug com memoria institucional\"\nwhen_to_use = [\"a reproducible defect\"]\n\n[node.verify]\ntype = \"gate\"\n"
        );
        let (_, short) = adw_description(bare).expect("bare");
        let (_, long) = adw_description(&rich).expect("rich");
        assert!(
            long.len() > short.len(),
            "the [purpose] block must widen the document: {} vs {}",
            long.len(),
            short.len()
        );
    }

    #[test]
    fn adw_document_survives_a_spec_with_no_description() {
        // A flow whose header comment carries the prose is still indexable.
        let src = "# scout-perpetuo — perpetual exploration that feeds the factory with tickets.\n[adw]\nname = \"scout\"\nentry = \"cycle\"\n";
        let (name, doc) = adw_description(src).expect("adw");
        assert_eq!(name.as_deref(), Some("scout"));
        assert!(doc.contains("perpetual exploration"), "{doc}");
    }

    #[test]
    fn mines_shell_header_after_shebang() {
        let src = "#!/usr/bin/env bash\n# safe-clean — surgical cargo target cleanup with anti-live-build gates.\n# Second line.\nset -euo pipefail\n";
        let got = shell_header(src).expect("shell header");
        assert!(got.contains("surgical cargo target cleanup"), "{got}");
    }

    #[test]
    fn display_path_collapses_home() {
        let Some(home) = home::home_dir() else { return };
        let p = home.join(".claude/skills/x/y.py");
        assert_eq!(display_path(&p), "~/.claude/skills/x/y.py");
    }

    #[test]
    fn provenance_distinguishes_skill_from_project() {
        let Some(home) = home::home_dir() else { return };
        assert_eq!(
            provenance_of(&home.join(".claude/skills/pdf-anthropic/scripts/a.py")),
            "skill:pdf-anthropic"
        );
        assert_eq!(
            provenance_of(&home.join("projects/konverter/scripts/a.py")),
            "project:konverter"
        );
    }

    #[test]
    fn keywords_split_stem_and_bundle() {
        let Some(home) = home::home_dir() else { return };
        let p = home.join(".claude/skills/pdf-anthropic/scripts/fill_pdf_form.py");
        let kws = keywords_for(&p, "skill:pdf-anthropic");
        for expected in ["fill", "pdf", "form", "anthropic"] {
            assert!(
                kws.contains(&expected.to_string()),
                "missing {expected}: {kws:?}"
            );
        }
    }

    #[test]
    fn mines_documented_python_functions_and_classes() {
        let src = "\ndef render_map(data):\n    \"\"\"Render the module dependency graph as an SVG map.\"\"\"\n    pass\n\nclass PdfBuilder:\n    \"\"\"Assemble a professional PDF from HTML templates.\"\"\"\n    pass\n";
        let syms = python_symbols(src);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"render_map"), "{names:?}");
        assert!(names.contains(&"PdfBuilder"), "{names:?}");
        assert!(
            syms[0].purpose.contains("dependency graph"),
            "{}",
            syms[0].purpose
        );
    }

    #[test]
    fn undocumented_and_private_symbols_are_not_indexed() {
        // No purpose prose → nothing to rank on; a leading underscore is private.
        let src = "def helper(x):\n    return x\n\ndef _internal(y):\n    \"\"\"Does something private but well described here.\"\"\"\n    pass\n";
        assert!(
            python_symbols(src).is_empty(),
            "{:?}",
            python_symbols(src).len()
        );
    }

    #[test]
    fn indented_definitions_are_skipped_as_detail() {
        let src = "class A:\n    def method(self):\n        \"\"\"A method that does a well described thing.\"\"\"\n        pass\n";
        let syms = python_symbols(src);
        assert!(
            syms.iter().all(|s| s.name != "method"),
            "methods are detail, not portfolio entries: {:?}",
            syms.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn mines_documented_rust_pub_items_past_attributes() {
        let src = "/// Compute the blast radius of a symbol across the workspace.\n#[must_use]\npub fn blast_radius(sym: &str) -> usize { 0 }\n\n/// A ranked capability returned by the portfolio query.\npub struct ScoredThing;\n\npub fn undocumented() {}\n";
        let syms = rust_symbols(src);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"blast_radius"), "{names:?}");
        assert!(names.contains(&"ScoredThing"), "{names:?}");
        assert!(
            !names.contains(&"undocumented"),
            "no doc → not indexed: {names:?}"
        );
    }

    #[test]
    fn symbol_cap_bounds_the_corpus() {
        let mut src = String::new();
        for i in 0..40 {
            src.push_str(&format!(
                "def f{i}(x):\n    \"\"\"A well described function number {i} doing work.\"\"\"\n    pass\n\n"
            ));
        }
        assert_eq!(python_symbols(&src).len(), MAX_SYMBOLS_PER_FILE);
    }

    #[test]
    fn stub_docstrings_do_not_enter_the_corpus() {
        // "Command-line interface." is 23 chars: past MIN_PURPOSE_LEN but not a
        // purpose. Measured 2026-08-08: such stubs outranked real artifacts.
        let dir = std::env::temp_dir().join(format!("portfolio-stub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let f = dir.join("t.py");
        std::fs::write(
            &f,
            "def main():\n    \"\"\"Command-line interface.\"\"\"\n    pass\n\ndef render_map(d):\n    \"\"\"Render the module dependency graph as an SVG map for review.\"\"\"\n    pass\n",
        )
        .expect("write");
        let content = std::fs::read_to_string(&f).expect("read");
        let names: Vec<String> = mine_symbols(&f, &content)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert!(
            !names.contains(&"main".to_string()),
            "stub indexed: {names:?}"
        );
        assert!(
            names.contains(&"render_map".to_string()),
            "real purpose dropped: {names:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn symbol_ids_are_deterministic_and_distinct_from_the_file() {
        let file_id = CapabilityEntry::make_id(CapabilityKind::Script, "~/a/b.py");
        let sym_id = CapabilityEntry::make_symbol_id("~/a/b.py", "render_map");
        assert_ne!(file_id, sym_id);
        assert_eq!(sym_id, "symbol:~/a/b.py::render_map");
        assert_eq!(
            sym_id,
            CapabilityEntry::make_symbol_id("~/a/b.py", "render_map")
        );
    }

    #[test]
    fn skip_dirs_exclude_vendored_trees() {
        assert!(is_skipped(Path::new("/x/.venv/lib/foo.py")));
        assert!(is_skipped(Path::new("/x/node_modules/y.py")));
        assert!(!is_skipped(Path::new("/x/scripts/y.py")));
    }

    #[test]
    fn mining_a_real_tree_is_deterministic_and_inherits_bundle_prose() {
        // End-to-end over a temp bundle that reproduces the pdf-anthropic shape:
        // a SKILL.md with prose + a script with none.
        let dir = std::env::temp_dir().join(format!("portfolio-mine-{}", std::process::id()));
        let scripts = dir.join("scripts");
        std::fs::create_dir_all(&scripts).expect("mkdir");
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: pdfkit\ndescription: Toolkit for generating professional PDF documents from templates.\n---\n",
        )
        .expect("write skill");
        std::fs::write(scripts.join("fill_form.py"), "import sys\nprint(1)\n")
            .expect("write script");

        let roots = vec![dir.clone()];
        let a = mine(&roots);
        let b = mine(&roots);
        assert_eq!(a, b, "mining must be deterministic");

        let script = a
            .iter()
            .find(|e| e.display_path.ends_with("fill_form.py"))
            .expect("script mined via inheritance");
        assert!(script.purpose_inherited, "should inherit from SKILL.md");
        assert!(
            script.purpose.contains("professional PDF"),
            "{}",
            script.purpose
        );
        assert_eq!(
            script
                .entry_point
                .as_deref()
                .map(|s| s.starts_with("python3")),
            Some(true)
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
