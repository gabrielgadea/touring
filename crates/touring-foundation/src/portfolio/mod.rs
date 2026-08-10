//! Capability Portfolio — prior-art discovery keyed by **purpose**, not by name.
//!
//! # Why this exists
//!
//! Before creating an artifact (a script that draws a map, a professional PDF
//! generator), the agent should see what already solves that purpose. The
//! symbol index cannot answer this: it is keyed by identifier, so
//! `touring tantivy search "prior art"` returns `art_root` (a fuzz shell) and
//! `with_prior` (a Bayesian predictor) — lexically perfect, semantically
//! unrelated.
//!
//! The signal exists but in an unindexed field. Measured 2026-08-08 over 3.881
//! Python scripts across `~/.claude/skills` and `~/projects/*/scripts`: **96%
//! carry purpose prose** (module docstring or `argparse` description), mean 411
//! characters. The portfolio indexes *that* field.
//!
//! # The anti-anchor contract
//!
//! A bare ranked list makes the agent reuse whatever ranked first. Every answer
//! therefore carries three sections ([`PortfolioAnswer`]):
//!
//! | section | role |
//! |---|---|
//! | `prior_art` | candidates **with provenance and evidence** — never bare paths |
//! | `gaps` | what the prior art does **not** cover for this intent |
//! | `external` | the external lens to consult (Context7 library + the question) |
//!
//! and it demands a [`Verdict`]: reuse, extend, supersede, or create-new. Naming
//! the gap is what invites superseding; without it the injection is an anchor.
//! An empty `prior_art` is a valid, honest answer — the portfolio never pads.
//!
//! The miner that populates the index lives in `touring-server`
//! (`portfolio::miner`) because it needs a filesystem walker; everything here is
//! pure, so the PreToolUse hook can query the portfolio without that weight.

pub mod feedback;
pub mod lexicon;
pub mod query;
pub mod store;

/// Optional semantic re-rank on top of the lexical ranking.
///
/// BM25 over purpose prose answers most intents well (measured 2026-08-08), but
/// it cannot see that "draw a diagram" and "render a chart" are the same wish.
/// A scorer closes that gap.
///
/// The trait lives here, in the light crate, while the implementation lives
/// wherever the embedding model does — so the PreToolUse hook can query the
/// portfolio without linking an ML runtime. It is also the seam that gives the
/// long-orphaned `touring_intelligence::rl::aco::template_library::EmbeddingStore`
/// its first real implementor (REGRA #0).
pub trait SemanticScorer: Send + Sync {
    /// Similarity of two texts in `[0.0, 1.0]`; `0.0` when it cannot be computed.
    fn score(&self, a: &str, b: &str) -> f64;
}

use serde::{Deserialize, Serialize};

/// What kind of artifact a capability record points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    /// An executable script (`.py`, `.sh`).
    Script,
    /// A source module whose header documents its purpose (`.rs`).
    Module,
    /// A documented function, class, struct or trait inside a file.
    ///
    /// Finer grain than [`Self::Script`]: answers "is there already a function
    /// that does X?", which the name-keyed symbol index cannot, because it
    /// matches identifiers rather than the prose that states purpose.
    Symbol,
    /// A skill bundle (`SKILL.md` frontmatter).
    Skill,
    /// A declarative agent workflow (`adw-library/*.toml`).
    Adw,
    /// A prose document that describes a strategy (cookbook, reference).
    Doc,
}

impl CapabilityKind {
    /// Stable lowercase tag used in ids and JSON.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Symbol => "symbol",
            Self::Module => "module",
            Self::Skill => "skill",
            Self::Adw => "adw",
            Self::Doc => "doc",
        }
    }
}

/// What is known about whether this artifact actually works.
///
/// Every field is an `Option` on purpose: **absence is displayed, not hidden**.
/// "no known test" must reach the reader, because the worst outcome of a
/// portfolio is reusing something broken because it merely ranked well.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    /// A sibling or in-file test was found.
    pub has_tests: Option<bool>,
    /// Age of the artifact in days at index time.
    pub modified_days_ago: Option<u64>,
    /// Verdict recorded the last time this intent was served (P3 feedback loop).
    pub prior_verdict: Option<Verdict>,
    /// RL reward attached to that prior verdict, when one exists.
    pub reward: Option<f64>,
}

impl Evidence {
    /// One-line human rendering that states absences explicitly.
    #[must_use]
    pub fn summary(&self) -> String {
        let tests = match self.has_tests {
            Some(true) => "tem teste",
            Some(false) => "sem teste conhecido",
            None => "cobertura não avaliada",
        };
        let age = self
            .modified_days_ago
            .map_or_else(|| "idade desconhecida".to_string(), |d| format!("modificado há {d}d"));
        match (&self.prior_verdict, self.reward) {
            (Some(v), Some(r)) => format!("{tests}; {age}; veredito anterior: {} (reward {r:.2})", v.tag()),
            (Some(v), None) => format!("{tests}; {age}; veredito anterior: {}", v.tag()),
            _ => format!("{tests}; {age}; nunca escolhido antes"),
        }
    }
}

/// The decision the agent must record after consulting the portfolio.
///
/// Requiring a verdict — rather than offering a suggestion — is what keeps the
/// portfolio from becoming either an anchor (blind reuse) or decoration (blind
/// reinvention). The verdict is persisted and becomes evidence for next time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Use the prior art as-is.
    Reuse,
    /// Build on it, changing it in place.
    Extend,
    /// Replace it — the prior art is inadequate and should be retired.
    Supersede,
    /// Nothing relevant exists; create from scratch.
    CreateNew,
}

impl Verdict {
    /// Stable lowercase tag for JSON, ids, and memory keys.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Reuse => "reuse",
            Self::Extend => "extend",
            Self::Supersede => "supersede",
            Self::CreateNew => "create_new",
        }
    }

    /// Parse from the CLI/memory tag; `None` for anything unrecognized.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "reuse" | "reusar" => Some(Self::Reuse),
            "extend" | "estender" => Some(Self::Extend),
            "supersede" | "superar" => Some(Self::Supersede),
            "create_new" | "create-new" | "criar-novo" | "novo" => Some(Self::CreateNew),
            _ => None,
        }
    }

    /// Every variant, for exhaustive rendering and tests.
    #[must_use]
    pub fn all() -> [Self; 4] {
        [Self::Reuse, Self::Extend, Self::Supersede, Self::CreateNew]
    }
}

/// One artifact in the portfolio, indexed by the prose that states its purpose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityEntry {
    /// Deterministic id: `"<kind>:<display_path>"` — derived from the canonical
    /// path, never from insertion order or a random source (REGRA #17).
    pub id: String,
    /// Path with `$HOME` collapsed to `~`, so records are portable and readable.
    pub display_path: String,
    /// What sort of artifact this is.
    pub kind: CapabilityKind,
    /// Short human name (file stem, skill name, or ADW name).
    pub name: String,
    /// The mined purpose prose — the field BM25 actually ranks.
    pub purpose: String,
    /// Source language / format tag (`python`, `rust`, `shell`, `markdown`, `toml`).
    pub language: String,
    /// How to invoke it, when that is derivable.
    pub entry_point: Option<String>,
    /// Where it came from (skill bundle, project) — the provenance line.
    pub provenance: String,
    /// Extra high-value terms (name fragments, bundle name) used for field boost.
    pub keywords: Vec<String>,
    /// What is known about whether it works.
    pub evidence: Evidence,
    /// True when the purpose was inherited from the bundle rather than the file
    /// itself — surfaced so a reader knows the description is about the bundle.
    pub purpose_inherited: bool,
}

impl CapabilityEntry {
    /// Build the deterministic id for a `(kind, display_path)` pair.
    #[must_use]
    pub fn make_id(kind: CapabilityKind, display_path: &str) -> String {
        format!("{}:{}", kind.tag(), display_path)
    }

    /// Deterministic id for a symbol inside a file: `"symbol:<path>::<name>"`.
    ///
    /// Derived from canonical name + location, never from walk order (REGRA #17).
    #[must_use]
    pub fn make_symbol_id(display_path: &str, symbol: &str) -> String {
        format!("{}:{display_path}::{symbol}", CapabilityKind::Symbol.tag())
    }
}

/// A scored candidate returned by a portfolio query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredCapability {
    /// The entry itself.
    pub entry: CapabilityEntry,
    /// BM25 score with field boost applied. Absolute scale is meaningless; only
    /// the ordering and the relation to the noise floor matter.
    pub score: f64,
}

/// The full three-section answer to an intent — the anti-anchor contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioAnswer {
    /// The intent as asked.
    pub intent: String,
    /// Ranked prior art above the noise floor. May legitimately be empty.
    pub prior_art: Vec<ScoredCapability>,
    /// What the prior art does not cover — the invitation to supersede.
    pub gaps: Vec<String>,
    /// External lenses worth consulting before committing.
    pub external: Vec<ExternalLens>,
    /// The verdicts the caller must choose between.
    pub verdict_required: Vec<String>,
    /// How many entries were searched, so a thin answer is legible as thin.
    pub corpus_size: usize,
}

/// A pointer to knowledge outside the workspace.
///
/// Deliberately a *pointer*, not a fetch: resolving Context7 on every query
/// would cost a network round-trip and fail offline. The portfolio names the
/// question; paying for the answer is the caller's decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalLens {
    /// Where to look (`context7`, `web`).
    pub source: String,
    /// The library or topic to resolve.
    pub subject: String,
    /// The specific question to ask — never a placeholder.
    pub question: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_deterministic_and_kind_scoped() {
        let a = CapabilityEntry::make_id(CapabilityKind::Script, "~/x/y.py");
        let b = CapabilityEntry::make_id(CapabilityKind::Script, "~/x/y.py");
        let c = CapabilityEntry::make_id(CapabilityKind::Doc, "~/x/y.py");
        assert_eq!(a, b, "same inputs must yield the same id (REGRA #17)");
        assert_ne!(a, c, "kind participates in identity");
        assert_eq!(a, "script:~/x/y.py");
    }

    #[test]
    fn verdict_roundtrips_through_both_languages() {
        for v in Verdict::all() {
            assert_eq!(Verdict::parse(v.tag()), Some(v));
        }
        assert_eq!(Verdict::parse("reusar"), Some(Verdict::Reuse));
        assert_eq!(Verdict::parse("superar"), Some(Verdict::Supersede));
        assert_eq!(Verdict::parse("criar-novo"), Some(Verdict::CreateNew));
        assert_eq!(Verdict::parse("talvez"), None);
    }

    #[test]
    fn evidence_states_absence_instead_of_hiding_it() {
        // The whole point: a record with nothing known must SAY so.
        let empty = Evidence::default();
        let s = empty.summary();
        assert!(s.contains("não avaliada"), "{s}");
        assert!(s.contains("desconhecida"), "{s}");
        assert!(s.contains("nunca escolhido"), "{s}");

        let known = Evidence {
            has_tests: Some(false),
            modified_days_ago: Some(12),
            prior_verdict: Some(Verdict::Supersede),
            reward: Some(0.25),
        };
        let k = known.summary();
        assert!(k.contains("sem teste conhecido"), "{k}");
        assert!(k.contains("há 12d"), "{k}");
        assert!(k.contains("supersede"), "{k}");
    }
}
