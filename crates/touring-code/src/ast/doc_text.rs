//! Documentation comments as searchable text (2026-09-13).
//!
//! The search index held a code symbol by its NAME only, so a question asked in
//! words ("stream actor that batches upserts and commits on a timer") could reach
//! a file only through identifiers that happen to share a token — while the
//! author had already written the answer above the item (`/// …`) or at the top of
//! the module (`//! …`). This module reads those comments back, line-based and
//! language-aware, without a parse: the index needs text, not a tree.
//!
//! | language | item doc | module doc |
//! |---|---|---|
//! | Rust | `///` block above the item (attributes skipped) | leading `//!` block |
//! | TypeScript / JavaScript / Java / C / C++ | `/** … */` or `//` block above | leading `/** … */` or `//` block |
//! | Go | `//` block above | `//` block above `package` |
//! | Python | docstring after the `def`/`class` line | module docstring |

/// Character cap of one extracted comment.
const DOC_TEXT_MAX_CHARS: usize = 2_000;

/// Collapses whitespace and caps by characters (never mid code point).
fn collapse(parts: &[String]) -> Option<String> {
    let text: String = parts
        .iter()
        .flat_map(|p| p.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ");
    let capped: String = text.chars().take(DOC_TEXT_MAX_CHARS).collect();
    (!capped.is_empty()).then_some(capped)
}

/// Whether `line` is an attribute or decorator the doc comment sits above.
fn is_attribute(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("#[") || t.starts_with('@')
}

/// The `/** … */`-or-`//` block ending right above index `end` (exclusive).
fn c_style_block_above(lines: &[&str], mut end: usize) -> Option<String> {
    while end > 0 && is_attribute(lines[end - 1]) {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let last = lines[end - 1].trim();
    let mut parts: Vec<String> = Vec::new();
    if last.ends_with("*/") {
        let mut i = end;
        while i > 0 {
            i -= 1;
            let t = lines[i].trim();
            let body = t
                .trim_start_matches("/**")
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim_start_matches('*')
                .trim();
            parts.push(body.to_string());
            if t.starts_with("/*") {
                break;
            }
        }
    } else {
        let mut i = end;
        while i > 0 && lines[i - 1].trim_start().starts_with("//") {
            i -= 1;
            parts.push(
                lines[i]
                    .trim_start()
                    .trim_start_matches('/')
                    .trim()
                    .to_string(),
            );
        }
    }
    parts.reverse();
    collapse(&parts)
}

/// The `///` block right above 1-based `line` (Rust), attributes skipped.
fn rust_item_doc(lines: &[&str], line: usize) -> Option<String> {
    let mut end = line.checked_sub(1)?;
    while end > 0 && is_attribute(lines[end - 1]) {
        end -= 1;
    }
    let mut parts: Vec<String> = Vec::new();
    let mut i = end;
    while i > 0 {
        let t = lines[i - 1].trim_start();
        match t.strip_prefix("///") {
            Some(rest) if !rest.starts_with('/') => parts.push(rest.trim().to_string()),
            _ => break,
        }
        i -= 1;
    }
    parts.reverse();
    collapse(&parts)
}

/// The docstring opening on the first non-blank line after 1-based `start`.
fn python_docstring_after(lines: &[&str], start: usize) -> Option<String> {
    let mut i = start;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    let first = lines.get(i)?.trim();
    let quote = ["\"\"\"", "'''"]
        .into_iter()
        .find(|q| first.starts_with(q))?;
    let opened = &first[quote.len()..];
    if let Some(end) = opened.find(quote) {
        return collapse(&[opened[..end].to_string()]);
    }
    let mut parts = vec![opened.to_string()];
    for next in lines.iter().skip(i + 1) {
        if let Some(end) = next.find(quote) {
            parts.push(next[..end].to_string());
            return collapse(&parts);
        }
        parts.push((*next).to_string());
    }
    collapse(&parts)
}

/// The documentation comment of the item defined at 1-based `line` of `content`,
/// for a file of `language` (the names `extension_to_language` produces).
#[must_use]
pub fn item_doc_comment(content: &str, language: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if line == 0 || line > lines.len() {
        return None;
    }
    match language {
        "rust" => rust_item_doc(&lines, line),
        "python" => python_docstring_after(&lines, line),
        "typescript" | "javascript" | "java" | "c" | "cpp" | "go" => {
            c_style_block_above(&lines, line - 1)
        }
        _ => None,
    }
}

/// The module-level documentation of `content`: Rust `//!`, Python module
/// docstring, or the leading comment block of a C-family / Go file.
#[must_use]
pub fn module_doc_comment(content: &str, language: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    match language {
        "rust" => {
            let parts: Vec<String> = lines
                .iter()
                .map(|l| l.trim_start())
                .skip_while(|l| l.is_empty() || l.starts_with("#!["))
                .map_while(|l| l.strip_prefix("//!").map(|r| r.trim().to_string()))
                .collect();
            collapse(&parts)
        }
        "python" => {
            let start = lines
                .iter()
                .position(|l| {
                    let t = l.trim();
                    !(t.is_empty() || t.starts_with('#'))
                })
                .unwrap_or(lines.len());
            python_docstring_after(&lines, start)
        }
        "go" => {
            let package = lines
                .iter()
                .position(|l| l.trim_start().starts_with("package "))?;
            c_style_block_above(&lines, package)
        }
        "typescript" | "javascript" | "java" | "c" | "cpp" => {
            let first = lines.iter().position(|l| !l.trim().is_empty())?;
            let t = lines[first].trim_start();
            if !(t.starts_with("/*") || t.starts_with("//")) {
                return None;
            }
            let end = if t.starts_with("/*") {
                lines
                    .iter()
                    .skip(first)
                    .position(|l| l.contains("*/"))
                    .map(|n| first + n + 1)?
            } else {
                first
                    + lines
                        .iter()
                        .skip(first)
                        .take_while(|l| l.trim_start().starts_with("//"))
                        .count()
            };
            c_style_block_above(&lines, end)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_item_docs_skip_attributes_and_stop_at_code_or_inner_docs() {
        let src = "//! crate docs\nfn other() {}\n/// Stream actor that batches upserts.\n/// Commits on a timer.\n#[must_use]\n#[inline]\npub fn run() {}\n//// not a doc\nfn bare() {}\n";
        assert_eq!(
            item_doc_comment(src, "rust", 7).as_deref(),
            Some("Stream actor that batches upserts. Commits on a timer.")
        );
        assert_eq!(
            item_doc_comment(src, "rust", 9),
            None,
            "`////` is a plain comment"
        );
        assert_eq!(
            item_doc_comment(src, "rust", 2),
            None,
            "`//!` is the module's, not the item's"
        );
        assert_eq!(item_doc_comment(src, "rust", 0), None);
        assert_eq!(item_doc_comment(src, "rust", 99), None);
    }

    #[test]
    fn rust_module_docs_are_the_leading_inner_block() {
        let src = "#![deny(missing_docs)]\n//! Unified search with RRF fusion.\n//!\n//! Combines rankings.\nuse std::fmt;\n//! late, not module docs\n";
        assert_eq!(
            module_doc_comment(src, "rust").as_deref(),
            Some("Unified search with RRF fusion. Combines rankings.")
        );
        assert_eq!(module_doc_comment("use x;\n", "rust"), None);
    }

    #[test]
    fn python_docstrings_single_and_multi_line() {
        let src = "#!/usr/bin/env python3\n\"\"\"Retrieval bench for companion roots.\n\nFrozen questions.\n\"\"\"\nimport json\n\ndef build():\n    '''Pick spans.'''\n    return 1\n\nclass Runner:\n\n    \"\"\"Runs\n    queries.\"\"\"\n";
        assert_eq!(
            module_doc_comment(src, "python").as_deref(),
            Some("Retrieval bench for companion roots. Frozen questions.")
        );
        assert_eq!(
            item_doc_comment(src, "python", 8).as_deref(),
            Some("Pick spans.")
        );
        assert_eq!(
            item_doc_comment(src, "python", 12).as_deref(),
            Some("Runs queries.")
        );
        assert_eq!(
            item_doc_comment(src, "python", 6),
            None,
            "an import has no docstring"
        );
    }

    #[test]
    fn c_family_and_go_blocks() {
        let ts = "/**\n * Fuse rankings.\n * @param k constant\n */\n@decorator\nexport function fuse() {}\n// line one\n// line two\nconst x = 1;\n";
        assert_eq!(
            item_doc_comment(ts, "typescript", 6).as_deref(),
            Some("Fuse rankings. @param k constant")
        );
        assert_eq!(
            item_doc_comment(ts, "typescript", 9).as_deref(),
            Some("line one line two")
        );
        assert_eq!(
            module_doc_comment(ts, "typescript").as_deref(),
            Some("Fuse rankings. @param k constant")
        );
        let go = "// Package wiring resolves imports.\npackage wiring\n\n// Resolve finds a target.\nfunc Resolve() {}\n";
        assert_eq!(
            module_doc_comment(go, "go").as_deref(),
            Some("Package wiring resolves imports.")
        );
        assert_eq!(
            item_doc_comment(go, "go", 5).as_deref(),
            Some("Resolve finds a target.")
        );
        assert_eq!(
            item_doc_comment(go, "markdown", 1),
            None,
            "unknown languages carry no doc"
        );
    }

    #[test]
    fn a_comment_longer_than_the_cap_is_cut_by_characters() {
        let long = format!("/// {}\nfn f() {{}}\n", "ação ".repeat(DOC_TEXT_MAX_CHARS));
        let doc = item_doc_comment(&long, "rust", 2).expect("doc");
        assert_eq!(doc.chars().count(), DOC_TEXT_MAX_CHARS);
    }
}
