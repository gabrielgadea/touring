//! Language × Feature capability matrix.
//!
//! Honest gap reporting — every entry reflects what Touring actually supports.

use super::tiers::{Language, Tier};
use serde::{Deserialize, Serialize};

/// Individual capability that can be supported per language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Full AST parsing and traversal.
    Ast,
    /// Symbol resolution (find definition, references).
    Symbols,
    /// Quality metrics (cognitive complexity, SLOC, Halstead).
    Quality,
    /// Wiring intelligence (orphan detection, integration scoring).
    Wiring,
    /// Cognitive analysis (module coupling, fan-in/fan-out).
    Cognitive,
    /// Syntax highlighting (syntect).
    Highlight,
    /// Fuzzy search (tantivy BM25).
    FuzzySearch,
    /// Refactor assists (extract function, rename, etc.).
    Assists,
    /// Cross-reference analysis (call graph, type deps).
    CrossRef,
}

/// Maturity level for a capability in a given language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupportLevel {
    /// Fully implemented and tested.
    Full,
    /// Works but with known gaps documented in the matrix.
    Partial,
    /// Proof-of-concept; may have rough edges.
    Experimental,
    /// Not implemented for this language.
    None,
}

/// A single capability entry for one language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageCapability {
    /// The capability being described.
    pub capability: Capability,
    /// Maturity level for this capability.
    pub level: SupportLevel,
    /// Short human-readable note on gaps or status.
    pub note: String,
}

impl LanguageCapability {
    fn new(capability: Capability, level: SupportLevel, note: &'static str) -> Self {
        Self {
            capability,
            level,
            note: note.to_owned(),
        }
    }
}

/// Full disclosure record for one language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageSupport {
    /// Language identifier.
    pub language: Language,
    /// Which tier this language belongs to.
    pub tier: Tier,
    /// All capability entries.
    pub capabilities: Vec<LanguageCapability>,
}

/// One capability cell in the table below: `(capability, support level, note)`.
type Cap = (Capability, SupportLevel, &'static str);

impl LanguageSupport {
    /// Build a language record from a compact `(capability, level, note)` table.
    /// Keeps the capability data as a dense, auditable literal while the
    /// `Self { .. }` / `LanguageCapability::new` scaffolding lives in one place.
    fn entry(language: Language, tier: Tier, caps: &[Cap]) -> Self {
        Self {
            language,
            tier,
            capabilities: caps
                .iter()
                .map(|&(capability, level, note)| LanguageCapability::new(capability, level, note))
                .collect(),
        }
    }

    /// Build the full capability matrix for every supported language.
    pub fn all() -> Vec<Self> {
        use Capability::*;
        use SupportLevel::*;

        vec![
            // ── Tier 1 ──────────────────────────────────────────────────────
            Self::entry(
                Language::Rust,
                Tier::Tier1,
                &[
                    (Ast, Full, "syntree parser — full reach"),
                    (Symbols, Full, "rs symbols via rust-analyzer infra"),
                    (Quality, Full, "Halstead + MI + cognitive CC"),
                    (Wiring, Full, "full orphan/wiring graph"),
                    (Cognitive, Full, "fan-in/out, modularity"),
                    (Highlight, Full, "syntect via syntect crate"),
                    (FuzzySearch, Full, "tantivy BM25 indexed"),
                    (Assists, Full, "10 assist kinds via touring-assists"),
                    (CrossRef, Full, "call graph via rust-analyzer"),
                ],
            ),
            Self::entry(
                Language::TypeScript,
                Tier::Tier1,
                &[
                    (Ast, Full, "tree-sitter TypeScript"),
                    (Symbols, Full, "tsc sys: root_info + nav"),
                    (Quality, Full, "Halstead + complexity"),
                    (Wiring, Full, "wiring graph active"),
                    (Cognitive, Full, "fan-in/out active"),
                    (Highlight, Full, "syntect JS/TS grammars"),
                    (FuzzySearch, Full, "tantivy indexed"),
                    (Assists, Partial, "rename + extract — partial list"),
                    (CrossRef, Full, "call graph via tsc"),
                ],
            ),
            // ── Tier 2 ──────────────────────────────────────────────────────
            Self::entry(
                Language::Python,
                Tier::Tier2,
                &[
                    (Ast, Full, "tree-sitter Python"),
                    (Symbols, Full, "tsc sys: root_info + nav"),
                    (Quality, Full, "Halstead + complexity"),
                    (Wiring, Partial, "no orphan graph yet"),
                    (Cognitive, Partial, "fan-in/out partial"),
                    (Highlight, Full, "syntect Python grammar"),
                    (FuzzySearch, Full, "tantivy indexed"),
                    (Assists, Partial, "basic assists only"),
                    (CrossRef, Partial, "import-level only"),
                ],
            ),
            Self::entry(
                Language::Go,
                Tier::Tier2,
                &[
                    (Ast, Full, "tree-sitter Go"),
                    (Symbols, Full, "go list + parse packages"),
                    (Quality, Full, "Halstead + complexity"),
                    (Wiring, Partial, "no orphan graph yet"),
                    (Cognitive, Partial, "fan-in/out partial"),
                    (Highlight, Full, "syntect Go grammar"),
                    (FuzzySearch, Full, "tantivy indexed"),
                    (Assists, Partial, "rename only"),
                    (CrossRef, Partial, "import-level only"),
                ],
            ),
            Self::entry(
                Language::C,
                Tier::Tier2,
                &[
                    (Ast, Full, "tree-sitter C (C18)"),
                    (Symbols, Partial, "global symbols only"),
                    (Quality, Full, "Halstead + complexity"),
                    (Wiring, None, "not yet wired"),
                    (Cognitive, Partial, "file-level only"),
                    (Highlight, Full, "syntect C grammar"),
                    (FuzzySearch, Full, "tantivy indexed"),
                    (Assists, None, "not yet implemented"),
                    (CrossRef, Partial, "header deps only"),
                ],
            ),
            // ── Tier 3 ──────────────────────────────────────────────────────
            Self::entry(
                Language::Kotlin,
                Tier::Tier3,
                &[
                    (Ast, Full, "tree-sitter Kotlin"),
                    (Symbols, Partial, "basic symbols only"),
                    (Quality, Partial, "basic metrics"),
                    (Wiring, None, "not yet wired"),
                    (Cognitive, None, "not yet implemented"),
                    (Highlight, Partial, "basic tokenization"),
                    (FuzzySearch, Partial, "experimental"),
                    (Assists, None, "not yet implemented"),
                    (CrossRef, None, "not yet implemented"),
                ],
            ),
            Self::entry(
                Language::Swift,
                Tier::Tier3,
                &[
                    (Ast, Full, "tree-sitter Swift"),
                    (Symbols, Partial, "basic symbols only"),
                    (Quality, Partial, "basic metrics"),
                    (Wiring, None, "not yet wired"),
                    (Cognitive, None, "not yet implemented"),
                    (Highlight, Partial, "basic tokenization"),
                    (FuzzySearch, Partial, "experimental"),
                    (Assists, None, "not yet implemented"),
                    (CrossRef, None, "not yet implemented"),
                ],
            ),
            Self::entry(
                Language::Java,
                Tier::Tier3,
                &[
                    (Ast, Full, "tree-sitter Java"),
                    (Symbols, Partial, "basic symbols only"),
                    (Quality, Partial, "basic metrics"),
                    (Wiring, None, "not yet wired"),
                    (Cognitive, None, "not yet implemented"),
                    (Highlight, Partial, "basic tokenization"),
                    (FuzzySearch, Partial, "experimental"),
                    (Assists, None, "not yet implemented"),
                    (CrossRef, None, "not yet implemented"),
                ],
            ),
            // ── Tier 4 ──────────────────────────────────────────────────────
            Self::entry(
                Language::Ruby,
                Tier::Tier4,
                &[
                    (Ast, Experimental, "tree-sitter Ruby — partial"),
                    (Symbols, None, "not yet implemented"),
                    (Quality, None, "not yet implemented"),
                    (Wiring, None, "not yet wired"),
                    (Cognitive, None, "not yet implemented"),
                    (Highlight, Partial, "basic tokenization"),
                    (FuzzySearch, None, "not yet implemented"),
                    (Assists, None, "not yet implemented"),
                    (CrossRef, None, "not yet implemented"),
                ],
            ),
            Self::entry(
                Language::Php,
                Tier::Tier4,
                &[
                    (Ast, Experimental, "tree-sitter PHP — partial"),
                    (Symbols, None, "not yet implemented"),
                    (Quality, None, "not yet implemented"),
                    (Wiring, None, "not yet wired"),
                    (Cognitive, None, "not yet implemented"),
                    (Highlight, Partial, "basic tokenization"),
                    (FuzzySearch, None, "not yet implemented"),
                    (Assists, None, "not yet implemented"),
                    (CrossRef, None, "not yet implemented"),
                ],
            ),
        ]
    }
}
