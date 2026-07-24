//! Wave v4.29.0 — curated risk patterns per language.
//!
//! These pattern sets feed `pre_read::AstGrepRiskSignalLayer`. Each entry
//! identifies a code-level construct that a careful reviewer would flag as
//! "warrants attention before editing":
//!
//! Rust uses unwrap, panic, todo, unimplemented.
//! Python uses dynamic-execution and untrusted-deserialization markers.
//! JS/TS uses eval and Function constructor.
//! Go uses panic.
//!
//! Patterns deliberately exclude false-positive carriers — ast-grep's
//! structural matcher distinguishes string-literal occurrences and comments
//! from real call sites.

use touring_code::polyglot::Lang;

use crate::ast_grep_signal::{PatternEntry, PatternSet};

/// Defensive Rust patterns — production hot-spots.
pub const RUST_RISK: PatternSet = PatternSet {
    set_id: "risk_rust_v1",
    language: Lang::Rust,
    patterns: &[
        PatternEntry {
            id: "unwrap",
            pattern: "$X.unwrap()",
            label: "unwrap",
        },
        PatternEntry {
            id: "panic",
            pattern: "panic!($$$)",
            label: "panic",
        },
        PatternEntry {
            id: "todo",
            pattern: "todo!()",
            label: "todo",
        },
        PatternEntry {
            id: "unimplemented",
            pattern: "unimplemented!()",
            label: "unimplemented",
        },
    ],
};

/// Defensive Python patterns.
pub const PYTHON_RISK: PatternSet = PatternSet {
    set_id: "risk_python_v1",
    language: Lang::Python,
    patterns: &[
        PatternEntry {
            id: "eval",
            pattern: "eval($$$)",
            label: "eval",
        },
        PatternEntry {
            id: "exec_call",
            pattern: "exec($$$)",
            label: "exec",
        },
        PatternEntry {
            id: "os_system",
            pattern: "os.system($$$)",
            label: "os.system",
        },
        PatternEntry {
            id: "pickle_loads",
            pattern: "pickle.loads($$$)",
            label: "pickle.loads",
        },
    ],
};

/// Defensive JavaScript / TypeScript patterns.
pub const JS_RISK: PatternSet = PatternSet {
    set_id: "risk_js_v1",
    language: Lang::JavaScript,
    patterns: &[
        PatternEntry {
            id: "eval",
            pattern: "eval($$$)",
            label: "eval",
        },
        PatternEntry {
            id: "function_ctor",
            pattern: "new Function($$$)",
            label: "Function ctor",
        },
    ],
};

/// Defensive Go patterns.
pub const GO_RISK: PatternSet = PatternSet {
    set_id: "risk_go_v1",
    language: Lang::Go,
    patterns: &[PatternEntry {
        id: "panic",
        pattern: "panic($$$)",
        label: "panic",
    }],
};

/// Resolve a pattern set for a [`Lang`]. Returns `None` for unsupported
/// languages so the caller can silently skip enrichment.
pub fn pattern_set_for(lang: Lang) -> Option<&'static PatternSet> {
    match lang {
        Lang::Rust => Some(&RUST_RISK),
        Lang::Python => Some(&PYTHON_RISK),
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => Some(&JS_RISK),
        Lang::Go => Some(&GO_RISK),
        _ => None,
    }
}

/// Resolve a [`Lang`] from a file path's extension.
pub fn lang_for_path(path: &std::path::Path) -> Option<Lang> {
    let ext = path.extension()?.to_str()?;
    let lang = match ext {
        "rs" => Lang::Rust,
        "py" => Lang::Python,
        "js" | "mjs" | "cjs" => Lang::JavaScript,
        "ts" => Lang::TypeScript,
        "tsx" => Lang::Tsx,
        "go" => Lang::Go,
        _ => return None,
    };
    Some(lang)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn pattern_set_for_rust_returns_rust_risk() {
        let pset = pattern_set_for(Lang::Rust).expect("rust set must exist");
        assert_eq!(pset.set_id, "risk_rust_v1");
        assert!(pset.patterns.iter().any(|p| p.id == "unwrap"));
    }

    #[test]
    fn pattern_set_for_python_returns_python_risk() {
        let pset = pattern_set_for(Lang::Python).expect("python set must exist");
        assert_eq!(pset.set_id, "risk_python_v1");
    }

    #[test]
    fn pattern_set_for_typescript_aliases_javascript() {
        let pset = pattern_set_for(Lang::TypeScript).expect("ts must alias js set");
        assert_eq!(pset.set_id, "risk_js_v1");
    }

    #[test]
    fn pattern_set_for_bash_returns_none() {
        assert!(pattern_set_for(Lang::Bash).is_none());
    }

    #[test]
    fn lang_for_path_resolves_rust() {
        assert_eq!(lang_for_path(Path::new("foo.rs")), Some(Lang::Rust));
    }

    #[test]
    fn lang_for_path_resolves_python() {
        assert_eq!(lang_for_path(Path::new("foo.py")), Some(Lang::Python));
    }

    #[test]
    fn lang_for_path_resolves_typescript_variants() {
        assert_eq!(lang_for_path(Path::new("a.ts")), Some(Lang::TypeScript));
        assert_eq!(lang_for_path(Path::new("a.tsx")), Some(Lang::Tsx));
    }

    #[test]
    fn lang_for_path_resolves_javascript_variants() {
        assert_eq!(lang_for_path(Path::new("a.js")), Some(Lang::JavaScript));
        assert_eq!(lang_for_path(Path::new("a.mjs")), Some(Lang::JavaScript));
        assert_eq!(lang_for_path(Path::new("a.cjs")), Some(Lang::JavaScript));
    }

    #[test]
    fn lang_for_path_unknown_extension_returns_none() {
        assert!(lang_for_path(Path::new("a.txt")).is_none());
        assert!(lang_for_path(Path::new("README")).is_none());
    }
}
