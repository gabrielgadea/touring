//! Search Index Preprocessing — NLP pipeline for .claude/ documents
//!
//! Reutiliza padrões NLP existentes:
//! - CodeAwareTokenizer: tokenização camelCase/snake_case/path-aware
//! - ClaudeDocExtractor: extração de metadados por tipo de documento

pub mod code_tokenizer;

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use unicode_segmentation::UnicodeSegmentation;

// ─── Stop Words ────────────────────────────────────────────────────────────

static PT_BR_STOPS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "de", "da", "do", "das", "dos", "em", "no", "na", "nos", "nas", "para", "por", "com",
        "sem", "que", "qual", "como", "quando", "uma", "um", "uns", "umas", "este", "esta", "esse",
        "essa", "aquele", "aquela", "não", "mais", "muito", "também", "outro", "ser", "ter",
        "estar", "haver", "fazer", "poder", "dever", "ao", "aos", "às", "pela", "pelo", "pelas",
        "pelos",
    ]
    .iter()
    .copied()
    .collect()
});

static CODE_STOPS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "self", "this", "return", "import", "from", "def", "class", "if", "else", "elif", "for",
        "while", "try", "except", "fn", "let", "mut", "pub", "use", "mod", "impl", "struct",
        "const", "var", "function", "async", "await", "export", "true", "false", "null", "none",
        "and", "or", "not",
    ]
    .iter()
    .copied()
    .collect()
});

// ─── CamelCase Splitting ──────────────────────────────────────────────────

fn split_camel_words(s: &str) -> Vec<String> {
    if s.is_empty() {
        return vec![];
    }
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();

    // SAFETY: All accesses use indices bounded by `n` (chars.len()).
    // `i-1` is safe because the guard `i > 0` ensures it.
    // `i+1` is guarded by `i + 1 < n`.
    #[allow(clippy::indexing_slicing)]
    for i in 0..n {
        let c = chars[i];
        if i > 0 && c.is_uppercase() {
            let prev_lower = chars[i - 1].is_lowercase() || chars[i - 1].is_numeric();
            let prev_upper = chars[i - 1].is_uppercase();
            let next_lower = i + 1 < n && chars[i + 1].is_lowercase();
            if (prev_lower || (prev_upper && next_lower)) && !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
        }
        current.push(c);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words.into_iter().map(|w| w.to_lowercase()).collect()
}

// ─── Regex Patterns ─────────────────────────────────────────────────────────

static PY_SYMBOL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(?:def|class|async def)\s+(\w+)").expect("static regex is valid")
});

static RS_SYMBOL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(?:pub )?(?:fn|struct|enum|trait|impl)\s+(\w+)")
        .expect("static regex is valid")
});

static TRIGGER_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:use when|trigger|ativa|use para)\s*[:\-]?\s*(.+)")
        .expect("static regex is valid")
});

static DEP_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\.claude/[\w/\-\.]+").expect("static regex is valid"));

// ─── PreprocessedDoc ─────────────────────────────────────────────────────────

/// Documento preprocessado pronto para indexação Tantivy
#[derive(Debug, Clone)]
pub struct PreprocessedDoc {
    /// Source path of the document.
    pub path: String,
    /// File name of the document.
    pub name: String,
    /// Detected document type (e.g. `skill`, `hook`, `rule`).
    pub doc_type: String,
    /// Code-aware tokens extracted from the content.
    pub content_tokens: Vec<String>,
    /// Code symbols (fn/class/struct names) found in the content.
    pub symbols: Vec<String>,
    /// Trigger phrases extracted from the content.
    pub triggers: Vec<String>,
    /// `.claude/` dependency paths referenced by the content.
    pub dependencies: Vec<String>,
    /// Logical module the document belongs to.
    pub module: String,
}

// ─── CodeAwareTokenizer ──────────────────────────────────────────────────────

/// Tokenizer code-aware: camelCase + snake_case + path-aware
#[derive(Debug)]
pub struct CodeAwareTokenizer;

impl CodeAwareTokenizer {
    /// Create a new code-aware tokenizer.
    pub fn new() -> Self {
        Self
    }

    /// Tokenize `text` into deduplicated, lowercased, stop-word-filtered tokens.
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        let mut words = Vec::new();

        for word in text.unicode_words() {
            let parts: Vec<&str> = word.split(['_', '.']).collect();
            for part in parts {
                if part.is_empty() {
                    continue;
                }
                let camel_parts: Vec<String> = split_camel_words(part);

                if camel_parts.is_empty() {
                    self.add_word(&part.to_lowercase(), &mut words);
                } else {
                    for t in camel_parts {
                        self.add_word(&t, &mut words);
                    }
                }
            }
        }

        words.dedup();
        words
    }

    fn add_word(&self, word: &str, words: &mut Vec<String>) {
        if word.len() < 2 {
            return;
        }
        if PT_BR_STOPS.contains(word) || CODE_STOPS.contains(word) {
            return;
        }
        words.push(word.to_string());
    }
}

impl Default for CodeAwareTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

// ─── ClaudeDocExtractor ──────────────────────────────────────────────────────

/// Extrator de metadados de documentos `.claude/` por tipo e path.
#[derive(Debug)]
pub struct ClaudeDocExtractor;

impl ClaudeDocExtractor {
    /// Classify a document type from its `.claude/` path (skill/hook/rule/etc.).
    pub fn detect_doc_type(path: &str) -> String {
        if path.contains("/skills/") {
            "skill".to_string()
        } else if path.contains("/hooks/") && path.ends_with(".py") {
            "hook".to_string()
        } else if path.contains("/rules/") {
            "rule".to_string()
        } else if path.contains("/agents/") {
            "agent".to_string()
        } else if path.contains("/commands/") {
            "command".to_string()
        } else if path.contains("/plans/") || path.contains("/docs/plans/") {
            "plan".to_string()
        } else if path.ends_with("hooks.json") || path.ends_with(".mcp.json") {
            "config".to_string()
        } else {
            "unknown".to_string()
        }
    }

    /// Extract code symbol names from `content` using a per-doc-type regex.
    pub fn extract_symbols(content: &str, doc_type: &str) -> Vec<String> {
        let pattern = match doc_type {
            "hook" | "skill" => &*PY_SYMBOL,
            "config" => return vec![],
            _ => &*RS_SYMBOL,
        };

        pattern
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Extract trigger phrases (e.g. "use when ...") from `content`.
    pub fn extract_triggers(content: &str) -> Vec<String> {
        TRIGGER_PATTERN
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty() && s.len() < 200)
            .collect()
    }

    pub(crate) fn extract_dependencies(content: &str) -> Vec<String> {
        DEP_PATTERN
            .find_iter(content)
            .map(|m| m.as_str().to_string())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub(crate) fn detect_module(path: &str) -> String {
        if path.contains("/skills/antt-") {
            "skills/antt".to_string()
        } else if path.contains("/skills/") {
            "skills".to_string()
        } else if path.contains("/hooks/learning/") {
            "hooks/learning".to_string()
        } else if path.contains("/hooks/siac/") {
            "hooks/siac".to_string()
        } else if path.contains("/hooks/") {
            "hooks".to_string()
        } else if path.contains("/rules/antt/") {
            "rules/antt".to_string()
        } else if path.contains("/rules/") {
            "rules".to_string()
        } else if path.contains("/agents/") {
            "agents".to_string()
        } else if path.contains("/orchestration/") {
            "orchestration".to_string()
        } else if path.contains("/rust-core/") {
            "rust-core".to_string()
        } else {
            "other".to_string()
        }
    }
}

// ─── SearchPreprocessor ──────────────────────────────────────────────────────

/// Preprocessador principal: combina tokenização, extração e detecção de tipo.
#[derive(Debug)]
pub struct SearchPreprocessor {
    tokenizer: CodeAwareTokenizer,
}

impl SearchPreprocessor {
    /// Create a new preprocessor with a default code-aware tokenizer.
    pub fn new() -> Self {
        Self {
            tokenizer: CodeAwareTokenizer::new(),
        }
    }

    /// Preprocess a document into a `PreprocessedDoc` ready for Tantivy indexing.
    pub fn preprocess(&self, path: &str, content: &str) -> PreprocessedDoc {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let doc_type = ClaudeDocExtractor::detect_doc_type(path);
        let symbols = ClaudeDocExtractor::extract_symbols(content, &doc_type);
        let triggers = ClaudeDocExtractor::extract_triggers(content);
        let dependencies = ClaudeDocExtractor::extract_dependencies(content);
        let module = ClaudeDocExtractor::detect_module(path);
        let content_tokens = self.tokenizer.tokenize(content);

        PreprocessedDoc {
            path: path.to_string(),
            name,
            doc_type,
            content_tokens,
            symbols,
            triggers,
            dependencies,
            module,
        }
    }
}

impl Default for SearchPreprocessor {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t1_camel_case_splitting() {
        let t = CodeAwareTokenizer::new();
        let lexemes = t.tokenize("getUserById");
        assert!(lexemes.contains(&"get".to_string()));
        assert!(lexemes.contains(&"user".to_string()));
        assert!(lexemes.contains(&"id".to_string()));
    }

    #[test]
    fn t2_snake_case_splitting() {
        let t = CodeAwareTokenizer::new();
        let lexemes = t.tokenize("pipeline_runner_v2");
        assert!(lexemes.contains(&"pipeline".to_string()));
        assert!(lexemes.contains(&"runner".to_string()));
    }

    #[test]
    fn t3_path_splitting() {
        let t = CodeAwareTokenizer::new();
        let lexemes = t.tokenize("search_api.py");
        assert!(lexemes.contains(&"search".to_string()));
        assert!(lexemes.contains(&"api".to_string()));
    }

    #[test]
    fn t5_detect_doc_type_skill() {
        assert_eq!(
            ClaudeDocExtractor::detect_doc_type(".claude/skills/antt-legal-analyzer/SKILL.md"),
            "skill"
        );
    }

    #[test]
    fn t6_detect_doc_type_hook() {
        assert_eq!(
            ClaudeDocExtractor::detect_doc_type(".claude/hooks/indexing/warmup_index.py"),
            "hook"
        );
    }

    #[test]
    fn t10_extract_symbols_python() {
        let content = "def my_function():\n    pass\nclass MyClass:\n    pass";
        let symbols = ClaudeDocExtractor::extract_symbols(content, "hook");
        assert!(symbols.contains(&"my_function".to_string()));
        assert!(symbols.contains(&"MyClass".to_string()));
    }

    #[test]
    fn t14_full_pipeline() {
        let p = SearchPreprocessor::new();
        let doc = p.preprocess(
            ".claude/skills/antt-legal-analyzer/SKILL.md",
            "Use when analyzing legal documents.\ndef analyze_legal():\n    pass",
        );
        assert_eq!(doc.doc_type, "skill");
        assert_eq!(doc.module, "skills/antt");
        assert!(!doc.content_tokens.is_empty());
        assert!(doc.symbols.contains(&"analyze_legal".to_string()));
        assert!(!doc.triggers.is_empty());
        assert_eq!(doc.name, "SKILL.md");
    }
}
