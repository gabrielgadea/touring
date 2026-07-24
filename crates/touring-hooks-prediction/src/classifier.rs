//! CILA Intent Classifier — O(n) single-pass prompt classification via RegexSet.
//!
//! Classifies user prompts into CILA levels (L0-L6) to determine how
//! Claude Code should process the request.
//!
//! - L0 (Direct): Pure text response
//! - L1 (PAL): Simple computation
//! - L2 (Tool-Augmented): Search + MCP tools
//! - L3 (Pipelines): ANTT process analysis
//! - L4 (Agent Loops): Iterative analysis
//! - L5 (Self-Modifying): Parameter evolution
//! - L6 (Multi-Agent): Team of parallel agents
//!
//! All 79 patterns compiled into a single `RegexSet` for O(n) matching.
//! v29.5: +21 patterns for audit/E2E/orchestration/improvement + heuristic fallback.
//!
//! 100% parity with rust-core/src/hooks_accel/intent_classifier.rs

use lru::LruCache;
use regex::RegexSet;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// Level names indexed by CILA level number.
const LEVEL_NAMES: [&str; 7] = [
    "Direct",         // L0
    "PAL",            // L1
    "Tool-Augmented", // L2
    "Pipelines",      // L3
    "Agent Loops",    // L4
    "Self-Modifying", // L5
    "Multi-Agent",    // L6
];

/// Pattern table: (level, strategy, regex_pattern).
/// Ordered L6 → L1 (highest priority first). L0 is the default fallback.
/// Each pattern uses `(?i)` for case-insensitive matching (parity with Python `re.IGNORECASE`).
const PATTERN_TABLE: &[(u8, &str, &str)] = &[
    // L6: Multi-Agent (team_spawn) — 5 patterns
    // Note: More flexible pattern to catch "team of agents" even with prefix words
    (
        6,
        "team_spawn",
        r"(?i)(?:equipe|team|swarm)(?:\s+(?:de\s+)?(?:agentes?|agents?))?",
    ),
    (
        6,
        "team_spawn",
        r"(?i)(?:parallel|paralel|concurrent)\w*\s+(?:analys|analis)",
    ),
    (6, "team_spawn", r"(?i)spawn\s+(?:team|agents)"),
    (6, "team_spawn", r"(?i)--swarm\b"),
    (6, "team_spawn", r"(?i)--agents-team\b"),
    // L5: Self-Modifying (self_evolution) — 3 patterns
    (
        5,
        "self_evolution",
        r"(?i)\b(?:evol|evolv|evoluc|mutat)\w*\s+(?:paramet|config|skill)",
    ),
    (5, "self_evolution", r"(?i)\bdrift\s+detect"),
    (5, "self_evolution", r"(?i)\bauto[_\-]?evol"),
    // L4: ACO Orchestration — 7 patterns
    (4, "aco_orchestration", r"(?i)--aco\b"),
    (4, "aco_orchestration", r"(?i)\baco\s+completo\b"),
    (4, "aco_orchestration", r"(?i)\bciclo\s+aco\b"),
    (4, "aco_orchestration", r"(?i)\bintentspec\b"),
    (4, "aco_orchestration", r"(?i)\bobjectivespec\b"),
    (4, "aco_orchestration", r"(?i)\bgeneratorgraph\b"),
    (4, "aco_orchestration", r"(?i)\bgoaltracker\b"),
    // L4: Agent Loops — 4 patterns
    (
        4,
        "agent_loop",
        r"(?i)\b(?:analise|analysis|deep)\s+(?:completa|full|profunda|deep|iterativa|analysis)\b",
    ),
    (4, "agent_loop", r"(?i)\bmax[_\-]?(?:effort|esforc)\b"),
    (4, "agent_loop", r"(?i)\bopus46\b"),
    (4, "agent_loop", r"(?i)\bdeep\s+regulat"),
    // L4: Plan Execution — 4 patterns
    (
        4,
        "plan_execution",
        r"(?i)\b(?:implement|execut)[aeo]r?\s+(?:o\s+)?plano\b",
    ),
    (4, "plan_execution", r"(?i)\bplano\s+completo\b"),
    (4, "plan_execution", r"(?i)\bexecute\s+(?:the\s+)?plan\b"),
    (4, "plan_execution", r"(?i)\.claude/plans/"),
    // L4: Audit, E2E, Orchestration, Cross-validation — 15 patterns (v29.5)
    (4, "agent_loop", r"(?i)\bauditoria\s+cruzada\b"),
    (4, "agent_loop", r"(?i)\bcross[_-]?audit\b"),
    (4, "agent_loop", r"(?i)\bE2E\b"),
    (4, "agent_loop", r"(?i)\bend[_-]to[_-]end\b"),
    (4, "agent_loop", r"(?i)\bintegra(?:tion|[çc][aã]o)\s+test"),
    (
        4,
        "agent_loop",
        r"(?i)\bfull\s+(?:pipeline|workflow|integration|coverage)\b",
    ),
    (
        4,
        "agent_loop",
        r"(?i)\b(?:verif|valid)\w*\s+(?:tudo|all|everything|completo|complete)\b",
    ),
    (4, "agent_loop", r"(?i)\borchestrat\w+\b"),
    (4, "agent_loop", r"(?i)\borquestrar\b"),
    (4, "agent_loop", r"(?i)\bTACO[_-]?task\b"),
    (4, "agent_loop", r"(?i)\bprove\s+(?:that|que)\b"),
    (4, "agent_loop", r"(?i)\bcomprove\b"),
    (
        4,
        "agent_loop",
        r"(?i)\b(?:dois|two|m[uú]ltiplos|multiple)\s+problemas?\b",
    ),
    (4, "agent_loop", r"(?i)\bproblemas?\s+graves?\b"),
    (4, "agent_loop", r"(?i)\bprove\s+na\s+pr[aá]tica\b"),
    // L3: Pipelines — 11 patterns
    (3, "pipeline", r"(?i)\b50500\.\d{6}[/\-]\d{4}[/\-]\d{2}\b"),
    (3, "pipeline", r"(?i)\b50505\.\d{6}[/\-]\d{4}[/\-]\d{2}\b"),
    (3, "pipeline", r"(?i)\bpipeline[_\-]?runner\b"),
    (3, "pipeline", r"(?i)\brun\s+pipeline\b"),
    (3, "pipeline", r"(?i)\bfases?\s+F?\d+\s*[\-a]\s*F?\d+\b"),
    (3, "pipeline", r"(?i)\bprocesso\s+(?:administrativo|SEI)\b"),
    (3, "pipeline", r"(?i)\brelat[oó]rio\s+[àa]\s+diretoria\b"),
    (3, "pipeline", r"(?i)\bvoto\s+(?:deliberat|relator)\w*\b"),
    (3, "pipeline", r"(?i)\btermo\s+aditivo\b"),
    (3, "pipeline", r"(?i)\breequil[ií]brio\b"),
    (3, "pipeline", r"(?i)\bfator\s+D\b"),
    // L3: Code Implementation — 5 patterns
    (
        3,
        "code_implementation",
        r"(?i)\b(?:implement|criar|crie|build|construir)\s+(?:\S+\s+){0,3}(?:feature|modulo|sistema|module|system)\b",
    ),
    (
        3,
        "code_implementation",
        r"(?i)\brefa(?:ct|t)or(?:a[r]?|ing|ed|iza[r]?)?\b",
    ),
    (3, "code_implementation", r"(?i)\b(?:sprint|pln2|pln3)\b"),
    (3, "code_implementation", r"(?i)\b(?:TDD|test.driven)\b"),
    (
        3,
        "code_implementation",
        r"(?i)\bcr(?:ie|iar)\s+(?:o\s+)?(?:script|arquivo|file)\b",
    ),
    // L3: General Improvement, Debug, Investigation — 7 patterns (v29.5)
    (
        3,
        "code_implementation",
        r"(?i)\b(?:melhore|otimize|aprimore|aperfei[cç]oe)\b",
    ),
    (
        3,
        "code_implementation",
        r"(?i)\bimprove\s+(?:the\s+)?\w+\b",
    ),
    (3, "code_implementation", r"(?i)\b(?:debug|depurar)\s+\w+"),
    (3, "code_implementation", r"(?i)\bdiagnosticar?\b"),
    (3, "code_implementation", r"(?i)\binvestigar?\b"),
    (3, "code_implementation", r"(?i)\banalise\s+o\b"),
    // L2: Tool-Augmented — 10 patterns
    (
        2,
        "tool_use",
        r"(?i)\bbusca[r]?\s+(?:\S+\s+){0,3}(?:na|no|em|nos|sobre|por)\b",
    ),
    (2, "tool_use", r"(?i)\bsearch\b"),
    (2, "tool_use", r"(?i)\bpesquis[ae]\b"),
    (2, "tool_use", r"(?i)\bconsult[ae]\b"),
    (2, "tool_use", r"(?i)\bRAG\b"),
    (2, "tool_use", r"(?i)\bContext7\b"),
    (2, "tool_use", r"(?i)\bCipher\b"),
    (2, "tool_use", r"(?i)\bSerena\b"),
    (2, "tool_use", r"(?i)\bnotebook\s*(?:LM|query)\b"),
    (2, "tool_use", r"(?i)\bfind\s+(?:all|files?)\b"), // English find patterns
    // L1: PAL (Simple computation) — 9 patterns
    // Note: calcul[aeo]r? covers Portuguese/Spanish; calculate covers English
    (1, "compute", r"(?i)\b(?:calcul[aeo]r?|calculate)\b"),
    (1, "compute", r"(?i)\bconvert[aei]r?\b"),
    (1, "compute", r"(?i)\bformat[aeo]r?\b"),
    (1, "compute", r"(?i)\blist[aeo]r?\b"),
    (1, "compute", r"(?i)\bcont[aeo]r?\b"),
    (1, "compute", r"(?i)\bquant[ao]s?\b"),
    // Additional patterns for simple math questions like "What is 2+2?"
    (1, "compute", r"(?i)\bwhat\s+(?:is|'s)\s+\d"),
    (1, "compute", r"(?i)\bquanto\s+[ée]\s+\d"),
    (1, "compute", r"(?i)\bqual\s+[ée]\s+o\s+resultado\b"),
];

/// Result of CILA intent classification.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CILAResult {
    /// CILA level (0-6).
    pub level: u8,
    /// Human-readable level name.
    pub level_name: String,
    /// Routing strategy identifier.
    pub routing_strategy: String,
    /// Whether ANTT pipeline state is required.
    pub requires_pipeline: bool,
    /// Whether CODE-FIRST protocol is required.
    pub requires_code_first: bool,
    /// The regex pattern that matched (empty for L0 default).
    pub matched_pattern: String,
}

/// Cognitive technique hints for CILA levels.
#[derive(Clone, Debug, serde::Serialize)]
pub enum CognitiveTechnique {
    /// Direct response (L0).
    DirectResponse,
    /// Compute first (L1).
    ComputeFirst,
    /// Tool augmented (L2).
    ToolAugmented,
    /// ANTT pipeline check (L3).
    AnttPipelineCheck,
    /// Code-first discover (L1-L4).
    CodeFirstDiscover,
    /// Self validation (L2-L5).
    SelfValidation,
    /// ACO identity (L4-L5).
    AcoIdentity,
    /// Self modifying (L5).
    SelfModifying,
    /// Team coordination (L6).
    TeamCoordination,
}

impl CognitiveTechnique {
    /// Get technique for CILA level.
    pub fn for_level(level: u8) -> Vec<Self> {
        match level {
            0 => vec![Self::DirectResponse],
            1 => vec![Self::ComputeFirst, Self::CodeFirstDiscover],
            2 => vec![
                Self::ToolAugmented,
                Self::CodeFirstDiscover,
                Self::SelfValidation,
            ],
            3 => vec![
                Self::AnttPipelineCheck,
                Self::CodeFirstDiscover,
                Self::SelfValidation,
            ],
            4 => vec![
                Self::CodeFirstDiscover,
                Self::SelfValidation,
                Self::AcoIdentity,
            ],
            5 => vec![Self::SelfValidation, Self::AcoIdentity, Self::SelfModifying],
            6 => vec![Self::TeamCoordination],
            _ => vec![Self::DirectResponse],
        }
    }

    /// Get hint string for the technique.
    pub fn hint(&self) -> &'static str {
        match self {
            Self::DirectResponse => "direct_response",
            Self::ComputeFirst => "compute_first",
            Self::ToolAugmented => "tool_augmented",
            Self::AnttPipelineCheck => "antt_pipeline_check",
            Self::CodeFirstDiscover => "code_first_discover",
            Self::SelfValidation => "self_validation",
            Self::AcoIdentity => "aco_identity",
            Self::SelfModifying => "self_modifying",
            Self::TeamCoordination => "team_coordination",
        }
    }
}

/// CILA Level enumeration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum CILALevel {
    /// L0: Direct response.
    L0,
    /// L1: PAL (Program-Aided Language).
    L1,
    /// L2: Tool-Augmented.
    L2,
    /// L3: Pipelines.
    L3,
    /// L4: Agent Loops.
    L4,
    /// L5: Self-Modifying.
    L5,
    /// L6: Multi-Agent.
    L6,
}

impl CILALevel {
    /// Convert to numeric level.
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::L0 => 0,
            Self::L1 => 1,
            Self::L2 => 2,
            Self::L3 => 3,
            Self::L4 => 4,
            Self::L5 => 5,
            Self::L6 => 6,
        }
    }

    /// From numeric level.
    pub fn from_u8(level: u8) -> Self {
        match level {
            0 => Self::L0,
            1 => Self::L1,
            2 => Self::L2,
            3 => Self::L3,
            4 => Self::L4,
            5 => Self::L5,
            _ => Self::L6,
        }
    }
}

/// High-performance CILA intent classifier using RegexSet.
///
/// All 79 patterns are compiled into a single `RegexSet` for O(n) matching.
/// First match (highest level) wins, matching Python's priority semantics.
#[derive(Debug)]
pub struct IntentClassifier {
    regex_set: RegexSet,
}

impl Default for IntentClassifier {
    fn default() -> Self {
        Self::build()
    }
}

impl IntentClassifier {
    /// Build a new classifier with all CILA patterns compiled.
    pub fn build() -> Self {
        let patterns: Vec<&str> = PATTERN_TABLE.iter().map(|(_, _, p)| *p).collect();
        let regex_set = RegexSet::new(patterns).expect("All CILA patterns must be valid regex");
        Self { regex_set }
    }

    /// Create new classifier (alias for build).
    pub fn new() -> Self {
        Self::build()
    }

    /// Classify a prompt (core logic, no PyO3 dependency).
    pub(crate) fn classify_prompt(&self, prompt: &str) -> CILAResult {
        if prompt.trim().is_empty() {
            return CILAResult {
                level: 0,
                level_name: "Direct".to_string(),
                routing_strategy: "direct".to_string(),
                requires_pipeline: false,
                requires_code_first: false,
                matched_pattern: String::new(),
            };
        }

        let matches = self.regex_set.matches(prompt);
        // Find the highest level match (L6 > L5 > L4 > ... > L1)
        let mut best_match: Option<(u8, &str, &str)> = None;
        for idx in matches.iter() {
            // SAFETY: RegexSet.matches() returns indices within 0..PATTERN_TABLE.len()
            #[allow(clippy::indexing_slicing)]
            let (level, strategy, pattern_str) = PATTERN_TABLE[idx];
            if best_match.map_or(true, |(best_level, _, _)| level > best_level) {
                best_match = Some((level, strategy, pattern_str));
            }
        }

        if let Some((level, strategy, pattern_str)) = best_match {
            CILAResult {
                level,
                level_name: LEVEL_NAMES
                    .get(level as usize)
                    .unwrap_or(&"Unknown")
                    .to_string(),
                routing_strategy: strategy.to_string(),
                // ACO manages its own workflow — no ANTT pipeline required
                requires_pipeline: level >= 3 && strategy != "aco_orchestration",
                requires_code_first: level >= 1,
                matched_pattern: pattern_str.to_string(),
            }
        } else {
            // No regex matched — apply structural heuristics before defaulting to L0.
            let trimmed = prompt.trim();
            // Multiple numbered items = multi-step task → L4 Agent Loops
            let has_numbered_list = (trimmed.contains("\n1.") || trimmed.starts_with("1."))
                && (trimmed.contains("\n2.") || trimmed.contains(" 2."));
            let (hlevel, hstrategy, hpattern, hcode_first) = if has_numbered_list {
                (4u8, "agent_loop", "heuristic:numbered_list", true)
            } else {
                let word_count = trimmed.split_whitespace().count();
                if word_count > 50 {
                    // Long technical prompts need tool use → L2
                    (2u8, "tool_use", "heuristic:long_prompt", true)
                } else {
                    (0u8, "direct", "", false)
                }
            };
            CILAResult {
                level: hlevel,
                level_name: LEVEL_NAMES
                    .get(hlevel as usize)
                    .unwrap_or(&"Unknown")
                    .to_string(),
                routing_strategy: hstrategy.to_string(),
                requires_pipeline: false,
                requires_code_first: hcode_first,
                matched_pattern: hpattern.to_string(),
            }
        }
    }

    /// Classify a single prompt into a CILA level.
    ///
    /// Returns CILAResult with level, strategy, and flags.
    /// Typical execution: <1μs per prompt.
    pub fn classify(&self, prompt: &str) -> CILAResult {
        self.classify_prompt(prompt)
    }

    /// Batch classify multiple prompts.
    pub fn classify_batch(&self, prompts: &[String]) -> Vec<CILAResult> {
        prompts.iter().map(|p| self.classify_prompt(p)).collect()
    }

    /// Return the total number of compiled patterns.
    pub fn pattern_count(&self) -> usize {
        PATTERN_TABLE.len()
    }

    /// Get cognitive technique hints for a CILA level.
    pub fn get_techniques(level: u8) -> Vec<CognitiveTechnique> {
        CognitiveTechnique::for_level(level)
    }
}

/// Compute a fast hash of a string using FxHasher.
fn fx_hash_str(s: &str) -> u64 {
    let mut hasher = FxHasher::default();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Thread-safe LRU-cached wrapper around `IntentClassifier`.
///
/// Avoids re-running 48 RegexSet patterns on repeated or similar prompts.
/// Capacity: 256 entries (configurable). Hash: FxHasher for speed.
///
/// # Thread Safety
/// Internal `Mutex<LruCache>` ensures safe concurrent access.
/// The underlying `IntentClassifier` is immutable after construction.
#[derive(Debug)]
pub struct CachedIntentClassifier {
    inner: IntentClassifier,
    cache: Mutex<LruCache<u64, CILAResult>>,
}

/// Cache hit/miss statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    /// Number of classification lookups served from the cache.
    pub hits: u64,
    /// Number of classification lookups that missed the cache.
    pub misses: u64,
    /// Maximum number of entries the LRU cache can hold.
    pub capacity: usize,
    /// Current number of entries in the cache.
    pub len: usize,
}

impl CachedIntentClassifier {
    /// Create a new cached classifier with the given LRU capacity.
    pub fn new(capacity: usize) -> Self {
        let cap =
            NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(256).expect("256 is non-zero"));
        Self {
            inner: IntentClassifier::build(),
            cache: Mutex::new(LruCache::new(cap)),
        }
    }

    /// Create with default capacity (256 entries).
    pub fn default_capacity() -> Self {
        Self::new(256)
    }

    /// Classify a prompt, returning a cached result if available.
    pub fn classify(&self, prompt: &str) -> CILAResult {
        let key = fx_hash_str(prompt);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(cached) = cache.get(&key) {
            return cached.clone();
        }

        let result = self.inner.classify_prompt(prompt);
        cache.put(key, result.clone());
        result
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.clear();
    }

    /// Return the number of compiled patterns (delegates to inner).
    pub fn pattern_count(&self) -> usize {
        self.inner.pattern_count()
    }

    /// Return current cache length.
    pub fn len(&self) -> usize {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.len()
    }

    /// Return true if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the LRU cache capacity.
    pub fn capacity(&self) -> usize {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.cap().into()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    fn classifier() -> IntentClassifier {
        IntentClassifier::build()
    }

    // ── L0: Direct (default fallback) ──────────────────────────────────

    #[test]
    fn test_empty_string() {
        let r = classifier().classify_prompt("");
        assert_eq!(r.level, 0);
        assert_eq!(r.level_name, "Direct");
        assert_eq!(r.routing_strategy, "direct");
        assert!(!r.requires_pipeline);
        assert!(!r.requires_code_first);
        assert!(r.matched_pattern.is_empty());
    }

    #[test]
    fn test_whitespace_only() {
        let r = classifier().classify_prompt("   \t\n  ");
        assert_eq!(r.level, 0);
    }

    #[test]
    fn test_hello() {
        let r = classifier().classify_prompt("olá, como vai?");
        assert_eq!(r.level, 0);
        assert_eq!(r.routing_strategy, "direct");
    }

    #[test]
    fn test_random_text() {
        let r = classifier().classify_prompt("O tempo está bom hoje");
        assert_eq!(r.level, 0);
    }

    // ── L6: Multi-Agent ────────────────────────────────────────────────

    #[test]
    fn test_l6_equipe_agentes() {
        let r = classifier().classify_prompt("crie uma equipe de agentes paralela");
        assert_eq!(r.level, 6);
        assert_eq!(r.routing_strategy, "team_spawn");
        assert!(r.requires_pipeline);
        assert!(r.requires_code_first);
    }

    #[test]
    fn test_l6_swarm_flag() {
        let r = classifier().classify_prompt("execute --swarm completo");
        assert_eq!(r.level, 6);
        assert_eq!(r.routing_strategy, "team_spawn");
    }

    #[test]
    fn test_l6_agents_team_flag() {
        let r = classifier().classify_prompt("use --agents-team para análise");
        assert_eq!(r.level, 6);
    }

    #[test]
    fn test_l6_parallel_analysis() {
        // Portuguese "paralela" matches prefix "paralel" (not English "parallel" with double-l)
        let r = classifier().classify_prompt("paralela analysis com agentes");
        assert_eq!(r.level, 6);
    }

    #[test]
    fn test_l6_spawn_team() {
        let r = classifier().classify_prompt("spawn team of agents");
        assert_eq!(r.level, 6);
    }

    // ── L5: Self-Modifying ─────────────────────────────────────────────

    #[test]
    fn test_l5_evolucao_parametros() {
        // Pattern requires "evol...\s+config" without extra words between
        let r = classifier().classify_prompt("evolucao config do sistema");
        assert_eq!(r.level, 5);
        assert_eq!(r.routing_strategy, "self_evolution");
    }

    #[test]
    fn test_l5_drift_detect() {
        let r = classifier().classify_prompt("execute drift detection no modelo");
        assert_eq!(r.level, 5);
    }

    #[test]
    fn test_l5_auto_evol() {
        let r = classifier().classify_prompt("ativar auto-evol para skills");
        assert_eq!(r.level, 5);
    }

    // ── L4: ACO Orchestration ──────────────────────────────────────────

    #[test]
    fn test_l4_aco_flag() {
        let r = classifier().classify_prompt("execute --aco completo");
        assert_eq!(r.level, 4);
        assert_eq!(r.routing_strategy, "aco_orchestration");
        // ACO does NOT require pipeline
        assert!(!r.requires_pipeline);
        assert!(r.requires_code_first);
    }

    #[test]
    fn test_l4_aco_completo() {
        let r = classifier().classify_prompt("rodar aco completo agora");
        assert_eq!(r.level, 4);
        assert_eq!(r.routing_strategy, "aco_orchestration");
    }

    #[test]
    fn test_l4_intentspec() {
        let r = classifier().classify_prompt("crie um IntentSpec para validação");
        assert_eq!(r.level, 4);
        assert_eq!(r.routing_strategy, "aco_orchestration");
    }

    #[test]
    fn test_l4_goaltracker() {
        let r = classifier().classify_prompt("verificar GoalTracker composite");
        assert_eq!(r.level, 4);
    }

    // ── L4: Agent Loops ────────────────────────────────────────────────

    #[test]
    fn test_l4_analise_completa() {
        let r = classifier().classify_prompt("analise completa do processo");
        assert_eq!(r.level, 4);
        assert_eq!(r.routing_strategy, "agent_loop");
        assert!(r.requires_pipeline);
    }

    #[test]
    fn test_l4_max_effort() {
        // Pattern "max[_-]?effort" requires no space — underscore, hyphen, or nothing
        let r = classifier().classify_prompt("use max_effort para resolver");
        assert_eq!(r.level, 4);
        assert_eq!(r.routing_strategy, "agent_loop");
    }

    #[test]
    fn test_l4_opus46() {
        let r = classifier().classify_prompt("ative opus46 deep analysis");
        assert_eq!(r.level, 4);
    }

    #[test]
    fn test_l4_deep_regulat() {
        let r = classifier().classify_prompt("deep regulatory review");
        assert_eq!(r.level, 4);
    }

    // ── L3: Pipelines ──────────────────────────────────────────────────

    #[test]
    fn test_l3_processo_sei_50500() {
        let r = classifier().classify_prompt("analisar 50500.123456/2024-01");
        assert_eq!(r.level, 3);
        assert_eq!(r.routing_strategy, "pipeline");
        assert!(r.requires_pipeline);
    }

    #[test]
    fn test_l3_processo_sei_50505() {
        let r = classifier().classify_prompt("verificar 50505.654321/2023-15");
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_l3_pipeline_runner() {
        let r = classifier().classify_prompt("executar pipeline_runner no diretório");
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_l3_processo_administrativo() {
        let r = classifier().classify_prompt("processo administrativo pendente");
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_l3_relatorio_diretoria() {
        let r = classifier().classify_prompt("preparar relatório à diretoria");
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_l3_reequilibrio() {
        let r = classifier().classify_prompt("reequilíbrio econômico-financeiro");
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_l3_fator_d() {
        let r = classifier().classify_prompt("calcular fator D de desempenho");
        // "calcular" matches L1, but "fator D" matches L3 — L3 has lower index (higher priority)
        // Actually, L3 patterns come BEFORE L1 in the table, so L3 wins
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_l3_termo_aditivo() {
        let r = classifier().classify_prompt("análise do termo aditivo proposto");
        assert_eq!(r.level, 3);
    }

    // ── L2: Tool-Augmented ─────────────────────────────────────────────

    #[test]
    fn test_l2_buscar_na_base() {
        let r = classifier().classify_prompt("buscar na base de dados");
        assert_eq!(r.level, 2);
        assert_eq!(r.routing_strategy, "tool_use");
    }

    #[test]
    fn test_l2_buscar_with_intervening_words() {
        // "buscar documentos no SEI" should match L2 even with words between verb and preposition
        let r = classifier().classify_prompt("buscar documentos no SEI");
        assert_eq!(r.level, 2);
        assert_eq!(r.routing_strategy, "tool_use");
    }

    #[test]
    fn test_l2_buscar_informacoes_sobre() {
        let r = classifier().classify_prompt("buscar informações sobre o contrato");
        assert_eq!(r.level, 2);
        assert_eq!(r.routing_strategy, "tool_use");
    }

    #[test]
    fn test_l2_buscar_too_many_words_does_not_match() {
        // 4+ intervening words should NOT match the L2 buscar pattern
        let r = classifier().classify_prompt("buscar um dois tres quatro no sistema");
        assert_ne!(r.routing_strategy, "tool_use");
    }

    #[test]
    fn test_l2_rag() {
        let r = classifier().classify_prompt("consulte o RAG para encontrar");
        assert_eq!(r.level, 2);
    }

    #[test]
    fn test_l2_context7() {
        let r = classifier().classify_prompt("verificar docs via Context7");
        assert_eq!(r.level, 2);
    }

    // ── L1: PAL ────────────────────────────────────────────────────────

    #[test]
    fn test_l1_calcule() {
        let r = classifier().classify_prompt("calcule o VPL do projeto");
        assert_eq!(r.level, 1);
        assert_eq!(r.routing_strategy, "compute");
        assert!(!r.requires_pipeline);
        assert!(r.requires_code_first);
    }

    #[test]
    fn test_l1_quantos() {
        let r = classifier().classify_prompt("quantos processos existem?");
        assert_eq!(r.level, 1);
    }

    #[test]
    fn test_l1_converte() {
        let r = classifier().classify_prompt("converte para markdown");
        assert_eq!(r.level, 1);
    }

    // ── Edge Cases ─────────────────────────────────────────────────────

    #[test]
    fn test_case_insensitive_calcule() {
        let r = classifier().classify_prompt("CALCULE o total");
        assert_eq!(r.level, 1);
    }

    #[test]
    fn test_case_insensitive_swarm() {
        let r = classifier().classify_prompt("--SWARM");
        assert_eq!(r.level, 6);
    }

    #[test]
    fn test_unicode_accents_relatorio() {
        let r = classifier().classify_prompt("relatório à diretoria colegiada");
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_unicode_reequilibrio() {
        let r = classifier().classify_prompt("REEQUILÍBRIO do contrato");
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_priority_l6_over_l4() {
        // "equipe de agentes" matches L6, "analise completa" matches L4
        // L6 should win (lower index in pattern table)
        let r = classifier().classify_prompt("equipe de agentes para analise completa");
        assert_eq!(r.level, 6);
    }

    #[test]
    fn test_priority_l3_over_l1() {
        // "fator D" matches L3, "calcule" matches L1
        let r = classifier().classify_prompt("calcule o fator D");
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_very_long_string() {
        let long = "a".repeat(10000);
        let r = classifier().classify_prompt(&long);
        assert_eq!(r.level, 0);
    }

    #[test]
    fn test_aco_no_pipeline_required() {
        let r = classifier().classify_prompt("--aco completo");
        assert_eq!(r.level, 4);
        assert_eq!(r.routing_strategy, "aco_orchestration");
        assert!(!r.requires_pipeline, "ACO should NOT require pipeline");
    }

    #[test]
    fn test_agent_loop_requires_pipeline() {
        let r = classifier().classify_prompt("analise completa");
        assert_eq!(r.level, 4);
        assert_eq!(r.routing_strategy, "agent_loop");
        assert!(r.requires_pipeline, "Agent loop L4 should require pipeline");
    }

    #[test]
    fn test_pattern_count() {
        assert_eq!(PATTERN_TABLE.len(), 79); // 58 original + 21 new (v29.5: audit/E2E/orchestration/improvement)
    }

    #[test]
    fn test_level_names_complete() {
        assert_eq!(LEVEL_NAMES.len(), 7);
        assert_eq!(LEVEL_NAMES[0], "Direct");
        assert_eq!(LEVEL_NAMES[6], "Multi-Agent");
    }

    #[test]
    fn test_l3_fases_f1_f9() {
        let r = classifier().classify_prompt("executar fases F1-F9");
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_l2_pesquisa() {
        let r = classifier().classify_prompt("pesquise sobre o assunto");
        assert_eq!(r.level, 2);
    }

    #[test]
    fn test_l5_evolv_config() {
        let r = classifier().classify_prompt("evolving configuration parameters");
        assert_eq!(r.level, 5);
    }

    #[test]
    fn test_l3_voto_deliberativo() {
        let r = classifier().classify_prompt("voto deliberativo da diretoria");
        assert_eq!(r.level, 3);
    }

    // ── Cognitive Technique Tests ──────────────────────────────────────

    #[test]
    fn test_techniques_l0() {
        let techs = CognitiveTechnique::for_level(0);
        assert_eq!(techs.len(), 1);
        assert!(matches!(techs[0], CognitiveTechnique::DirectResponse));
    }

    #[test]
    fn test_techniques_l3() {
        let techs = CognitiveTechnique::for_level(3);
        assert!(
            techs
                .iter()
                .any(|t| matches!(t, CognitiveTechnique::AnttPipelineCheck))
        );
        assert!(
            techs
                .iter()
                .any(|t| matches!(t, CognitiveTechnique::CodeFirstDiscover))
        );
    }

    #[test]
    fn test_techniques_l6() {
        let techs = CognitiveTechnique::for_level(6);
        assert!(
            techs
                .iter()
                .any(|t| matches!(t, CognitiveTechnique::TeamCoordination))
        );
    }

    // ── CachedIntentClassifier Tests ─────────────────────────────────

    #[test]
    fn test_cached_hit_returns_same_result() {
        let cached = CachedIntentClassifier::default_capacity();
        let r1 = cached.classify("analise completa do processo");
        let r2 = cached.classify("analise completa do processo");
        assert_eq!(r1.level, r2.level);
        assert_eq!(r1.routing_strategy, r2.routing_strategy);
        assert_eq!(r1.level_name, r2.level_name);
        assert_eq!(r1.requires_pipeline, r2.requires_pipeline);
        assert_eq!(r1.requires_code_first, r2.requires_code_first);
        assert_eq!(r1.matched_pattern, r2.matched_pattern);
    }

    #[test]
    fn test_cached_populates_on_miss() {
        let cached = CachedIntentClassifier::default_capacity();
        assert!(cached.is_empty());
        cached.classify("calcule o VPL");
        assert_eq!(cached.len(), 1);
        cached.classify("buscar na base");
        assert_eq!(cached.len(), 2);
    }

    #[test]
    fn test_cached_same_prompt_no_growth() {
        let cached = CachedIntentClassifier::default_capacity();
        cached.classify("olá mundo");
        cached.classify("olá mundo");
        cached.classify("olá mundo");
        assert_eq!(cached.len(), 1);
    }

    #[test]
    fn test_cached_capacity_eviction() {
        // Use a tiny cache to verify LRU eviction
        let cached = CachedIntentClassifier::new(2);
        assert_eq!(cached.capacity(), 2);
        cached.classify("prompt_a");
        cached.classify("prompt_b");
        assert_eq!(cached.len(), 2);
        // This should evict "prompt_a" (least recently used)
        cached.classify("prompt_c");
        assert_eq!(cached.len(), 2);
    }

    #[test]
    fn test_cached_clear() {
        let cached = CachedIntentClassifier::default_capacity();
        cached.classify("hello");
        cached.classify("world");
        assert_eq!(cached.len(), 2);
        cached.clear();
        assert!(cached.is_empty());
    }

    #[test]
    fn test_cached_correctness_across_levels() {
        let cached = CachedIntentClassifier::default_capacity();
        assert_eq!(cached.classify("olá").level, 0); // L0
        assert_eq!(cached.classify("calcule 2+2").level, 1); // L1
        assert_eq!(cached.classify("buscar na base").level, 2); // L2
        assert_eq!(cached.classify("50500.123456/2024-01").level, 3); // L3
        assert_eq!(cached.classify("analise completa").level, 4); // L4
        assert_eq!(cached.classify("drift detect now").level, 5); // L5
        assert_eq!(cached.classify("--swarm").level, 6); // L6
        assert_eq!(cached.len(), 7);
    }

    #[test]
    fn test_cached_pattern_count_delegates() {
        let cached = CachedIntentClassifier::default_capacity();
        assert_eq!(cached.pattern_count(), PATTERN_TABLE.len());
    }

    // ── v29.5: New L4 patterns (audit/E2E/orchestration) ──────────────

    #[test]
    fn test_l4_dois_problemas_graves() {
        // The original failing prompt — was L0, must now be L4
        let r = classifier().classify_prompt(
            "dois problemas graves\n1. CILA classifier está péssimo\n2. Touring mcp tools ineficazes",
        );
        assert_eq!(
            r.level, 4,
            "multi-problem prompt must be L4, got L{}",
            r.level
        );
    }

    #[test]
    fn test_l4_cross_audit() {
        let r = classifier().classify_prompt("auditoria cruzada de todos os módulos");
        assert_eq!(r.level, 4);
    }

    #[test]
    fn test_l4_e2e_keyword() {
        let r = classifier().classify_prompt("verificar E2E completo do pipeline");
        assert_eq!(r.level, 4);
    }

    #[test]
    fn test_l4_end_to_end() {
        let r = classifier().classify_prompt("end-to-end integration test suite");
        assert_eq!(r.level, 4);
    }

    #[test]
    fn test_l4_orchestrate() {
        let r = classifier().classify_prompt("orchestrate all microservices startup");
        assert_eq!(r.level, 4);
    }

    #[test]
    fn test_l4_taco_task() {
        let r = classifier().classify_prompt("TACO-task implementar feature X com testes");
        assert_eq!(r.level, 4);
    }

    #[test]
    fn test_l4_prove_na_pratica() {
        let r = classifier().classify_prompt("prove na prática que o sistema funciona");
        assert_eq!(r.level, 4);
    }

    #[test]
    fn test_l3_improve_the() {
        let r = classifier().classify_prompt("improve the performance of the cache layer");
        assert_eq!(r.level, 3);
    }

    #[test]
    fn test_l3_debug_module() {
        let r = classifier().classify_prompt("debug the authentication module");
        assert_eq!(r.level, 3);
    }

    // ── v29.5: Heuristic fallback tests ───────────────────────────────

    #[test]
    fn test_heuristic_numbered_list_is_l4() {
        // Numbered list with no other pattern match → heuristic:numbered_list → L4
        let r = classifier().classify_prompt(
            "1. first unknown task\n2. second unknown task\n3. third unknown task",
        );
        assert_eq!(r.level, 4, "numbered list heuristic must yield L4");
        assert!(
            r.matched_pattern.contains("heuristic"),
            "must report heuristic pattern"
        );
    }

    #[test]
    fn test_heuristic_long_prompt_is_l2() {
        // >50 words, no pattern match → heuristic:long_prompt → L2
        let long = "word ".repeat(55);
        let r = classifier().classify_prompt(long.trim());
        assert_eq!(r.level, 2, "long unmatched prompt must be L2");
        assert!(
            r.matched_pattern.contains("heuristic"),
            "must report heuristic pattern"
        );
    }

    #[test]
    fn test_heuristic_short_unmatched_is_l0() {
        // Short prompt with no pattern match → L0 (direct response)
        let r = classifier().classify_prompt("xyz abc qrs");
        assert_eq!(r.level, 0);
    }
}
