//! E2E contract: the web UI and the served stylesheet stay in harmony.
//!
//! Born in the 2026-06-11 cross-audit, which found (a) 28 `db-*` classes the
//! Wave-1 dashboard referenced but no stylesheet defined (silent unstyled
//! render), and (b) `theme.rs` embedding a stale legacy stylesheet that
//! overrode the elite design at runtime via a late `<style>` tag.
//!
//! Test 1 — every design-system class cited in `src/web/{routes,components}`
//!          is defined in `touring-web/public/assets/styles/main.css`.
//! Test 2 — `theme.rs` embeds exactly the stylesheet Trunk serves.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Design-system prefixes under contract. Page-hook wrappers (`*-page`) are
/// intentionally out of scope: they are semantic selectors with no rules.
const PREFIXES: &[&str] = &[
    "el-", "ql-", "srch-", "mem-", "wir-", "hlt-", "orp-", "fed-", "qd-", "qr-", "ws-", "db-",
];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The directory of stylesheets Trunk links into the page. The design system
/// was split from a single `main.css` into `main`/`elite`/`pages`/`charts`/
/// `tokens` in the 2026-06-12 interface-elite wave, so a cited class may be
/// defined in any of them — the contract checks the union of all served CSS.
fn served_stylesheets_dir() -> PathBuf {
    manifest_dir().join("../touring-web/public/assets/styles")
}

/// Concatenate every served `.css` so the contract checks the full design
/// system, not just one file. Deterministic (sorted) for stable diagnostics.
fn served_stylesheets() -> String {
    let dir = served_stylesheets_dir();
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("served stylesheets dir missing at {}: {e}", dir.display()))
        .map(|e| e.expect("readable dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "css"))
        .collect();
    paths.sort();
    let mut css = String::new();
    for p in &paths {
        css.push_str(&fs::read_to_string(p).expect("readable stylesheet"));
        css.push('\n');
    }
    css
}

/// Extract every double-quoted string literal from Rust source. Good enough
/// for `view!` bodies: handles `\"` escapes, skips raw strings (none used).
fn string_literals(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut lit = String::new();
        while let Some(&n) = chars.peek() {
            chars.next();
            if n == '\\' {
                if let Some(&esc) = chars.peek() {
                    lit.push(esc);
                    chars.next();
                }
            } else if n == '"' {
                break;
            } else {
                lit.push(n);
            }
        }
        out.push(lit);
    }
    out
}

/// Class-like tokens (`[A-Za-z0-9_-]+`) within a string, filtered to the
/// design-system prefixes. `format!` placeholders (`{…}`) break tokens, so a
/// dynamic suffix never produces a half-token that passes the prefix filter.
fn class_tokens(s: &str) -> Vec<String> {
    s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .filter(|t| PREFIXES.iter().any(|p| t.starts_with(p)))
        .map(str::to_string)
        .collect()
}

fn collect_cited_classes(dir: &Path, cited: &mut BTreeSet<String>) {
    for entry in fs::read_dir(dir).expect("web source dir must exist") {
        let path = entry.expect("readable dir entry").path();
        if path.is_dir() {
            collect_cited_classes(&path, cited);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let src = fs::read_to_string(&path).expect("readable source file");
            for lit in string_literals(&src) {
                cited.extend(class_tokens(&lit));
            }
        }
    }
}

/// Selector tokens defined in the stylesheet: `.` followed by an identifier.
fn collect_defined_classes(css: &str) -> BTreeSet<String> {
    let mut defined = BTreeSet::new();
    let bytes = css.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_alphabetic() {
            let rest = &css[i + 1..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(rest.len());
            defined.insert(rest[..end].to_string());
        }
    }
    defined
}

#[test]
fn every_cited_design_system_class_is_defined_in_served_css() {
    let css = served_stylesheets();
    let defined = collect_defined_classes(&css);
    assert!(
        defined.len() > 200,
        "stylesheet parse degenerated: only {} classes found across served CSS",
        defined.len()
    );
    let mut cited = BTreeSet::new();
    collect_cited_classes(&manifest_dir().join("src/web/routes"), &mut cited);
    collect_cited_classes(&manifest_dir().join("src/web/components"), &mut cited);
    // Degeneration guard only (the real contract is `missing.is_empty()` below):
    // the UI currently cites ~85 prefixed design-system classes across 22 routes
    // + 19 components; keep the floor well under that so the guard catches a
    // broken scan (≈0) without false-failing on routine UI churn.
    assert!(
        cited.len() > 50,
        "citation scan degenerated: only {} classes found",
        cited.len()
    );
    let missing: Vec<&String> = cited.difference(&defined).collect();
    assert!(
        missing.is_empty(),
        "classes cited by the web UI but undefined in any served stylesheet ({}):\n  {}",
        served_stylesheets_dir().display(),
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn theme_embeds_the_served_stylesheet() {
    let theme_src =
        fs::read_to_string(manifest_dir().join("src/web/theme.rs")).expect("theme.rs must exist");
    let needle = r#"include_str!("../../../touring-web/public/assets/styles/main.css")"#;
    let count = theme_src.matches(needle).count();
    assert!(
        count >= 1,
        "theme.rs must embed the SAME stylesheet Trunk serves (single \
         inclusion point in css_vars); found {count} include_str references — \
         a drift here resurrects the legacy-override bug fixed in the \
         2026-06-11 audit"
    );
    assert!(
        theme_src.contains("theme.css_vars()"),
        "theme_signal() must source its injected CSS from css_vars() so the \
         inclusion point stays single"
    );
    assert!(
        !theme_src.contains(r#"include_str!("./styles/"#),
        "theme.rs reverted to the deleted legacy stylesheet directory"
    );
}
