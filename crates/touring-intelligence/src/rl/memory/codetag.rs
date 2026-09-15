// #tags: kind:module lang:rust domain:memory purpose:auto-tagging process:post-edit status:experimental
//! Codetag scanner — harvests `#tags:` anchors from source files into
//! snippet-level memories (F2 of the hashtag library).
//!
//! The convention (PEP 350 lineage): a comment line containing the literal
//! marker `#tags:` followed by space-separated `#facet:value` tokens anchors
//! the code block that follows it —
//!
//! ```text
//! // #tags: kind:snippet lang:rust purpose:blast-radius domain:wiring
//! pub fn blast_radius(...) { ... }
//! ```
//!
//! The marker works inside any comment syntax (`//`, `#`, `/* */`, `<!-- -->`)
//! because only the literal `#tags:` is scanned. The anchored block becomes a
//! `snippet` memory whose *value is the code itself*, so a tag query returns
//! the snippet without opening the file — the granular retrieval the library
//! exists for. Re-syncing a file tombstones anchors that disappeared (code is
//! the source of truth; memory follows it).
//!
//! Keys are deterministic: `snippet:{relpath}#L{line}` (REGRA #17 — the same
//! anchor re-derives the same entry across sessions).

use std::path::Path;

use super::tags::{self, ParsedTag, TagSource};

/// One `#tags:` anchor found in a source file.
#[derive(Debug, Clone)]
pub struct CodetagHit {
    /// 1-indexed line of the anchor comment.
    pub line: usize,
    /// Parsed tags from the anchor line (invalid tokens are skipped).
    pub tags: Vec<ParsedTag>,
    /// The extracted code block the anchor covers.
    pub block: String,
}

/// The literal marker scanned for (matched case-sensitively after trimming —
/// the convention is lowercase `#tags:`).
const MARKER: &str = "#tags:";

/// Hard cap on extracted block size — an anchor never swallows a whole file.
const MAX_BLOCK_LINES: usize = 200;

/// Parses the tags of one anchor line: everything after `#tags:` is treated
/// as whitespace-separated tag tokens; malformed ones are dropped (the linter
/// already warns at authoring time).
fn parse_anchor_line(line: &str) -> Option<Vec<ParsedTag>> {
    let idx = line.find(MARKER)?;
    let rest = &line[idx + MARKER.len()..];
    let tags: Vec<ParsedTag> = rest
        .split_whitespace()
        .filter_map(|tok| tags::parse_tag(tok).ok())
        .collect();
    if tags.is_empty() { None } else { Some(tags) }
}

/// Extracts the block an anchor covers, starting at `start` (0-indexed line
/// after the anchor). Rust-family files use brace matching from the first
/// `{`; Python uses indentation; everything else takes a paragraph (up to the
/// first blank line followed by a non-indented line) capped at
/// `MAX_BLOCK_LINES`.
fn extract_block(lang: &str, lines: &[&str], start: usize) -> String {
    let body: Vec<&str> = lines
        .iter()
        .skip(start)
        .skip_while(|l| l.trim().is_empty())
        .copied()
        .collect();
    if body.is_empty() {
        return String::new();
    }
    let taken = match lang {
        "rust" | "ts" | "js" => take_brace_block(&body),
        "python" | "bash" | "yaml" | "toml" => take_indent_block(&body),
        _ => take_paragraph(&body),
    };
    taken
        .into_iter()
        .take(MAX_BLOCK_LINES)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Brace-balanced extraction: from the first `{` until its match, or the
/// signature line alone when the item is a declaration (e.g. trait method).
/// Braces are counted on the line with string/char literals and line comments
/// stripped (A-1, cross-audit 2026-08-12: a `"}"` inside a `format!` arg used
/// to close the block early and silently truncate the snippet).
fn take_brace_block<'a>(body: &[&'a str]) -> Vec<&'a str> {
    let mut depth = 0i32;
    let mut opened = false;
    let mut out: Vec<&'a str> = Vec::new();
    for line in body {
        let stripped = strip_code_literals(line);
        let opens = stripped.matches('{').count() as i32;
        let closes = stripped.matches('}').count() as i32;
        if !opened && opens == 0 && line.trim_end().ends_with(';') {
            out.push(line);
            break; // declaration without body
        }
        depth += opens - closes;
        if opens > 0 {
            opened = true;
        }
        out.push(line);
        if opened && depth <= 0 {
            break;
        }
    }
    out
}

/// Strips string literals, char literals and `//` comments from one line so
/// structural scanning (brace counting) only sees real code. Lifetimes
/// (`'a`) are kept — distinguished from char literals by the closing quote:
/// `'x'`/`'\n'` is a char, `'a` is a lifetime. Multi-line constructs (raw
/// strings `r#"…"#`, block comments `/* */`) are out of scope for this
/// single-line pass — documented limitation, both are rare inside the
/// function bodies this extractor targets.
fn strip_code_literals(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_string = false;
    let mut iter = line.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if in_string {
            match c {
                '\\' => {
                    iter.next(); // escaped char inside the string
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '/' if iter.peek().is_some_and(|(_, n)| *n == '/') => break, // line comment
            '\'' => match char_literal_len(&line[i..]) {
                Some(len) => {
                    for _ in 0..len.saturating_sub(1) {
                        iter.next();
                    }
                    out.push(' ');
                }
                None => out.push(c), // lifetime/label — kept
            },
            _ => out.push(c),
        }
    }
    out
}

/// Length of a char literal starting at a `'` (`'x'` → 3, `'\n'` → 4), or
/// `None` when the quote opens a lifetime/label instead.
fn char_literal_len(rest_from_quote: &str) -> Option<usize> {
    let mut cs = rest_from_quote.chars();
    cs.next()?; // the opening quote
    Some(match cs.next()? {
        '\\' if cs.next().is_some() && cs.next() == Some('\'') => 4,
        c if cs.next() == Some('\'') => 1 + c.len_utf8() + 1,
        _ => return None,
    })
}

/// Indentation extraction: the anchored item's body is strictly more indented
/// than its opening line; the first non-blank line at or below the opening
/// indentation ends the block.
fn take_indent_block<'a>(body: &[&'a str]) -> Vec<&'a str> {
    let base = indent_of(body[0]);
    let mut out: Vec<&'a str> = vec![body[0]];
    for line in &body[1..] {
        if line.trim().is_empty() {
            out.push(line);
            continue;
        }
        if indent_of(line) <= base {
            break;
        }
        out.push(line);
    }
    out
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Paragraph extraction: contiguous non-blank lines (blank line ends it).
fn take_paragraph<'a>(body: &[&'a str]) -> Vec<&'a str> {
    body.iter()
        .copied()
        .take_while(|l| !l.trim().is_empty())
        .collect()
}

/// Scans `text` (the content of `path`) and returns every codetag anchor with
/// its extracted block. `lang` is derived from the file extension through the
/// same table the store-time derivation uses.
pub(crate) fn scan_source(path: &str, text: &str) -> Vec<CodetagHit> {
    let lang = path
        .rsplit('.')
        .next()
        .and_then(|ext| {
            let lower = ext.to_ascii_lowercase();
            tags::EXT_TO_LANG
                .iter()
                .find(|(e, _)| *e == lower)
                .map(|(_, lang)| *lang)
        })
        .unwrap_or("");
    let lines: Vec<&str> = text.lines().collect();
    let mut hits = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if let Some(tags) = parse_anchor_line(line) {
            let block = extract_block(lang, &lines, idx + 1);
            hits.push(CodetagHit {
                line: idx + 1,
                tags,
                block,
            });
        }
    }
    hits
}

/// Deterministic memory key for an anchor (REGRA #17).
pub(crate) fn snippet_key(relpath: &str, line: usize) -> String {
    format!("snippet:{relpath}#L{line}")
}

/// Outcome of syncing one file.
#[derive(Debug, Default)]
pub struct SyncReport {
    /// Anchors written/refreshed.
    pub upserted: usize,
    /// Stale snippet entries removed (anchor vanished from the source).
    pub tombstoned: usize,
}

/// Syncs one file's anchors into the memory store: each anchor becomes /
/// refreshes a `snippet` entry (value = the block, tags = anchor tags +
/// store-time derivation, source `code_sync`); snippet entries of this file
/// whose anchors disappeared are deleted (tombstone — code wins).
///
/// `relpath` is the project-relative path used in keys, so a sync from any
/// cwd re-derives the same keys.
pub fn sync_file(
    conn: &rusqlite::Connection,
    relpath: &str,
    text: &str,
) -> rusqlite::Result<SyncReport> {
    tags::ensure_tag_schema(conn)?;
    let hits = scan_source(relpath, text);
    let mut report = SyncReport::default();

    for hit in &hits {
        upsert_hit(conn, relpath, hit)?;
        report.upserted += 1;
    }

    // Tombstones: snippet entries of this file whose line no longer anchors.
    // Match by `file_path` (indexed), not a LIKE over the key prefix.
    let mut stmt = conn.prepare(
        "SELECT key FROM memory_entries WHERE entry_type = 'snippet' AND file_path = ?1",
    )?;
    let existing: Vec<String> = stmt
        .query_map(rusqlite::params![relpath], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let live: std::collections::HashSet<String> =
        hits.iter().map(|h| snippet_key(relpath, h.line)).collect();
    let key_prefix = format!("snippet:{relpath}#L");
    for key in existing {
        if key.starts_with(&key_prefix) && !live.contains(&key) {
            conn.execute(
                "DELETE FROM memory_tags WHERE entry_key = ?1",
                rusqlite::params![key],
            )?;
            conn.execute(
                "DELETE FROM memory_entries WHERE key = ?1",
                rusqlite::params![key],
            )?;
            report.tombstoned += 1;
        }
    }
    Ok(report)
}

/// Writes/refreshes one anchor: the snippet entry (value = header + block)
/// plus its full tag set — anchor tags and store-time derivation, all marked
/// `code_sync` so a later explicit tag on the same key is never clobbered.
fn upsert_hit(
    conn: &rusqlite::Connection,
    relpath: &str,
    hit: &CodetagHit,
) -> rusqlite::Result<()> {
    let key = snippet_key(relpath, hit.line);
    let header = format!("-- {relpath}:L{} (codetag-anchored snippet)\n", hit.line);
    let value = format!("{header}{}", hit.block);
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO memory_entries(key, value, tier, entry_type, access_count,
                                   last_accessed_at, created_at, file_path)
         VALUES (?1, ?2, 'local', 'snippet', 0, datetime('now'), ?3, ?4)
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            entry_type = excluded.entry_type,
            file_path = excluded.file_path",
        rusqlite::params![key, value, now, relpath],
    )?;
    // The anchor is canonical for this key's code-synced tags: re-derive the
    // set and replace only that stratum, preserving any `explicit` tags.
    let mut wanted: Vec<ParsedTag> = tags::derive_tags(&key, "snippet", Some(relpath));
    wanted.extend(hit.tags.iter().cloned());
    wanted.sort_by(|a, b| a.full_tag.cmp(&b.full_tag));
    wanted.dedup_by(|a, b| a.full_tag == b.full_tag);
    conn.execute(
        "DELETE FROM memory_tags WHERE entry_key = ?1 AND source = 'code_sync'",
        rusqlite::params![key],
    )?;
    for tag in &wanted {
        tags::upsert_tag(conn, &key, tag, TagSource::CodeSync)?;
    }
    Ok(())
}

/// Whether `relpath` still owns snippet entries in the store. This is the
/// cheap side of the tombstone contract: a file whose *last* anchor was
/// removed no longer contains the marker, so a marker-only fast path would
/// never re-sync it and the stale snippet would survive forever (D1,
/// cross-audit 2026-08-23). Callers use this to decide whether a markerless
/// file still needs a sync pass (which will tombstone everything).
pub fn has_snippet_entries(conn: &rusqlite::Connection, relpath: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(
        "SELECT 1 FROM memory_entries WHERE entry_type = 'snippet' AND file_path = ?1 LIMIT 1",
    )?;
    stmt.exists(rusqlite::params![relpath])
}

/// Source extensions the batch walker indexes.
pub const CODETAG_EXTENSIONS: &[&str] = &[
    "rs", "py", "sh", "bash", "ts", "js", "toml", "yaml", "yml", "md",
];

/// Walks `root` (respecting `.gitignore` via the `ignore` crate) and syncs
/// every source file that contains the marker. Fast path: files without the
/// literal `#tags:` substring are skipped before any parsing.
/// `walk_root` is where the scan happens; `key_root` is what snippet keys
/// are relative to. They differ when syncing a subdirectory: keys must stay
/// project-relative or two files with the same name in different dirs
/// collapse into one `snippet:<name>#L<n>` key and tombstone each other
/// (A-2, cross-audit 2026-08-12).
pub fn sync_tree(
    conn: &rusqlite::Connection,
    walk_root: &Path,
    key_root: &Path,
) -> std::io::Result<SyncReport> {
    let mut total = SyncReport::default();
    for entry in ignore::WalkBuilder::new(walk_root)
        .hidden(true)
        .build()
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !CODETAG_EXTENSIONS.contains(&ext) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = path
            .strip_prefix(key_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        // Fast path: no marker AND no surviving snippet entries → nothing to
        // upsert, nothing to tombstone. The second clause keeps the contract
        // "removing the codetag removes the memory" honest when the file's
        // LAST anchor was deleted (a marker-only skip would orphan it).
        if !text.contains(MARKER) && !has_snippet_entries(conn, &rel).unwrap_or(false) {
            continue;
        }
        if let Ok(report) = sync_file(conn, &rel, &text) {
            total.upserted += report.upserted;
            total.tombstoned += report.tombstoned;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    fn scans_rust_anchor_and_brace_block() {
        let src = "// header\n// #tags: kind:snippet purpose:blast-radius domain:wiring\npub fn blast() -> usize {\n    let x = 1;\n    x\n}\n\nfn other() {}\n";
        let hits = scan_source("src/wiring.rs", src);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 2);
        assert!(hits[0].tags.iter().any(|t| t.full_tag == "kind:snippet"));
        assert!(hits[0].block.contains("pub fn blast()"));
        assert!(hits[0].block.contains("x"));
        assert!(
            !hits[0].block.contains("fn other"),
            "block stops at brace match"
        );
    }

    #[rstest]
    fn scans_python_anchor_and_indent_block() {
        let src = "# #tags: kind:script lang:python\ndef render_map():\n    layers = []\n    return layers\n\nx = 1\n";
        let hits = scan_source("scripts/render_map.py", src);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].block.contains("def render_map():"));
        assert!(hits[0].block.contains("return layers"));
        assert!(!hits[0].block.contains("x = 1"), "block stops at dedent");
    }

    #[rstest]
    fn declaration_without_body_is_single_line() {
        let src = "// #tags: kind:snippet\ntrait T {\n    fn sig(&self) -> usize;\n}\n";
        let hits = scan_source("a.rs", src);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].block.contains("trait T"));
    }

    #[rstest]
    fn invalid_tokens_are_dropped_not_fatal() {
        let src = "// #tags: kind:snippet bogus-token #zzz:nope\nfn f() {}\n";
        let hits = scan_source("a.rs", src);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].tags.len(), 1);
    }

    #[rstest]
    fn line_without_marker_is_ignored() {
        let src = "// just a comment about tags: nothing\nfn f() {}\n";
        assert!(scan_source("a.rs", src).is_empty());
    }

    /// A-1 (cross-audit 2026-08-12): a `}` inside a string/char literal must
    /// not close the block early and truncate the snippet.
    #[rstest]
    fn braces_inside_literals_do_not_close_the_block() {
        let src = "// #tags: kind:snippet\npub fn render() -> String {\n    let s = \"}\";\n    let c = '{';\n    format!(\"{s}{c} chapter } done\")\n}\n\nfn next_fn() {}\n";
        let hits = scan_source("a.rs", src);
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].block.contains("format!"),
            "block must reach the last line of the fn: {}",
            hits[0].block
        );
        assert!(
            !hits[0].block.contains("next_fn"),
            "block must stop at the real close brace"
        );
    }

    /// Lifetimes are not char literals: `'a` must survive the stripper (a
    /// signature with generics still has its braces counted).
    #[rstest]
    fn lifetimes_survive_literal_stripping() {
        let src =
            "// #tags: kind:snippet\nfn parse<'a>(input: &'a str) -> &'a str {\n    input\n}\n";
        let hits = scan_source("a.rs", src);
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].block.contains("input"),
            "block complete: {}",
            hits[0].block
        );
    }

    /// A-2 (cross-audit 2026-08-12): syncing a SUBDIRECTORY must still write
    /// project-relative keys — two same-named files in different dirs can
    /// never share one snippet key.
    #[rstest]
    fn sync_tree_keys_are_key_root_relative() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project = tmp.path().join("proj");
        let sub = project.join("docs/guides");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join("tool.py"),
            "# #tags: kind:script\ndef f():\n    return 1\n",
        )
        .unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE memory_entries (
                key TEXT PRIMARY KEY, value TEXT NOT NULL,
                tier TEXT NOT NULL DEFAULT 'local',
                entry_type TEXT NOT NULL DEFAULT 'insight',
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                file_path TEXT
            );",
        )
        .unwrap();
        let report = sync_tree(&conn, &sub, &project).unwrap();
        assert_eq!(report.upserted, 1);
        let key: String = conn
            .query_row("SELECT key FROM memory_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            key, "snippet:docs/guides/tool.py#L1",
            "key must be project-relative, not walk-root-relative"
        );
    }

    /// D1 (cross-audit 2026-08-23): removing the LAST anchor of a file must
    /// still tombstone through the batch walker's own guard. A marker-only
    /// skip left the stale snippet alive forever — this test drives the real
    /// production path (`sync_tree`), not `sync_file` directly.
    #[rstest]
    fn sync_tree_tombstones_file_whose_last_anchor_was_removed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project = tmp.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let file = project.join("tool.py");
        std::fs::write(&file, "# #tags: kind:script\ndef f():\n    return 1\n").unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE memory_entries (
                key TEXT PRIMARY KEY, value TEXT NOT NULL,
                tier TEXT NOT NULL DEFAULT 'local',
                entry_type TEXT NOT NULL DEFAULT 'insight',
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                file_path TEXT
            );",
        )
        .unwrap();
        assert_eq!(sync_tree(&conn, &project, &project).unwrap().upserted, 1);

        // The last (only) anchor disappears from the source.
        std::fs::write(&file, "def f():\n    return 1\n").unwrap();
        let r2 = sync_tree(&conn, &project, &project).unwrap();
        assert_eq!(
            r2.tombstoned, 1,
            "markerless file with entries must tombstone"
        );
        let survivors: i64 = conn
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(survivors, 0, "code is the source of truth for snippets");
    }

    #[rstest]
    fn snippet_key_is_deterministic() {
        assert_eq!(snippet_key("src/a.rs", 42), "snippet:src/a.rs#L42");
        assert_eq!(snippet_key("src/a.rs", 42), snippet_key("src/a.rs", 42));
    }

    #[rstest]
    fn sync_file_roundtrip_and_tombstone() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE memory_entries (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                tier TEXT NOT NULL DEFAULT 'local',
                entry_type TEXT NOT NULL DEFAULT 'insight',
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                file_path TEXT
            );",
        )
        .unwrap();

        let v1 = "// #tags: kind:snippet purpose:map-rendering\nfn render() {}\n";
        let r1 = sync_file(&conn, "src/map.rs", v1).unwrap();
        assert_eq!(r1.upserted, 1);
        let stored: String = conn
            .query_row(
                "SELECT value FROM memory_entries WHERE key = 'snippet:src/map.rs#L1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored.contains("fn render()"));
        let tag_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_tags WHERE entry_key = 'snippet:src/map.rs#L1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(tag_count >= 3, "anchor tag + derived tags, got {tag_count}");

        // Anchor removed from the source → entry + tags tombstoned.
        let r2 = sync_file(&conn, "src/map.rs", "fn render() {}\n").unwrap();
        assert_eq!(r2.tombstoned, 1);
        let gone: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_entries WHERE key LIKE 'snippet:src/map.rs%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(gone, 0);
    }
}
