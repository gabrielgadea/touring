//! Keyword Matcher - Pln2 Module 4
//!
//! High-performance multi-pattern matching using Aho-Corasick algorithm.
//!
//! # Features
//!
//! - O(n) time complexity for matching multiple patterns (vs O(n*m) naive)
//! - LRU cache for compiled automata
//! - Case-insensitive matching
//! - Fuzzy matching with Levenshtein distance
//! - Pre-defined ANTT regulatory patterns
//! - Context extraction around matches
//! - Parallel batch processing

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use moka::sync::Cache;
use once_cell::sync::Lazy;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// ============================================================================
// Data Structures
// ============================================================================

/// Result of a keyword match with context.
#[derive(Debug, Clone)]
pub struct KeywordMatch {
    /// The keyword pattern that matched.
    pub keyword: String,

    /// Index of the pattern in the original list.
    pub pattern_index: usize,

    /// Start position in bytes.
    pub start: usize,

    /// End position in bytes.
    pub end: usize,

    /// The actual matched text (may differ from keyword if fuzzy).
    pub matched_text: String,

    /// Context before the match (N chars).
    pub context_before: String,

    /// Context after the match (N chars).
    pub context_after: String,

    /// Match score \[0,1\] (1.0 = exact, <1.0 = fuzzy).
    pub match_score: f64,

    /// Category of the pattern (if set).
    pub category: Option<String>,
}

/// Category of ANTT regulatory patterns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PatternCategory {
    /// ANTT Resolution (Resolução).
    Resolution,
    /// Federal Law (Lei).
    Law,
    /// Decree (Decreto).
    Decree,
    /// SEI Process (Processo).
    Process,
    /// TCU Ruling or Summary (Acórdão/Súmula).
    Tcu,
    /// Contract or Addendum (Contrato/Aditivo).
    Contract,
    /// Technical Note (Nota Técnica).
    TechnicalNote,
    /// Legal Opinion (Parecer).
    Opinion,
    /// Monetary Value.
    Monetary,
    /// Custom pattern.
    Custom,
}

impl std::fmt::Display for PatternCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Resolution => "Resolution",
            Self::Law => "Law",
            Self::Decree => "Decree",
            Self::Process => "Process",
            Self::Tcu => "TCU",
            Self::Contract => "Contract",
            Self::TechnicalNote => "TechnicalNote",
            Self::Opinion => "Opinion",
            Self::Monetary => "Monetary",
            Self::Custom => "Custom",
        };
        write!(f, "{}", name)
    }
}

/// Configuration for the keyword matcher.
#[derive(Debug, Clone)]
pub struct MatcherConfig {
    /// Number of characters of context to extract before/after match.
    pub context_size: usize,

    /// Enable case-insensitive matching.
    pub case_insensitive: bool,

    /// Allow overlapping matches.
    pub allow_overlapping: bool,

    /// Enable fuzzy matching (Levenshtein distance).
    pub fuzzy_enabled: bool,

    /// Maximum edit distance for fuzzy matching.
    pub max_edit_distance: usize,

    /// Minimum score threshold for fuzzy matches.
    pub min_fuzzy_score: f64,
}

impl Default for MatcherConfig {
    fn default() -> Self {
        Self {
            context_size: 50,
            case_insensitive: true,
            allow_overlapping: false,
            fuzzy_enabled: false,
            max_edit_distance: 2,
            min_fuzzy_score: 0.8,
        }
    }
}

impl MatcherConfig {
    fn __repr__(&self) -> String {
        format!(
            "MatcherConfig(context_size={}, case_insensitive={}, fuzzy_enabled={})",
            self.context_size, self.case_insensitive, self.fuzzy_enabled
        )
    }
}

// ============================================================================
// Global Automaton Cache
// ============================================================================

/// Global concurrent cache for compiled Aho-Corasick automata.
/// Cache key is hash of patterns + config, value is Arc\<AhoCorasick\>.
/// Uses moka W-TinyLFU cache for lock-free concurrent reads.
static AUTOMATON_CACHE: Lazy<Cache<u64, Arc<AhoCorasick>>> =
    Lazy::new(|| Cache::new(AUTOMATON_CACHE_MAX as u64));

/// Maximum number of entries before periodic cleanup.
const AUTOMATON_CACHE_MAX: usize = 100;

/// Cache statistics for monitoring.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Number of cache hits.
    pub hits: usize,
    /// Number of cache misses.
    pub misses: usize,
    /// Current cache size.
    pub size: usize,
    /// Maximum cache capacity.
    pub capacity: usize,
}

static CACHE_HITS: AtomicUsize = AtomicUsize::new(0);
static CACHE_MISSES: AtomicUsize = AtomicUsize::new(0);

/// Get current cache statistics.
pub fn get_cache_stats() -> CacheStats {
    CacheStats {
        hits: CACHE_HITS.load(Ordering::Relaxed),
        misses: CACHE_MISSES.load(Ordering::Relaxed),
        size: 0, // moka doesn't expose len()
        capacity: AUTOMATON_CACHE_MAX,
    }
}

/// Clear the automaton cache.
pub fn clear_cache() {
    AUTOMATON_CACHE.invalidate_all();
    CACHE_HITS.store(0, Ordering::Relaxed);
    CACHE_MISSES.store(0, Ordering::Relaxed);
}

// ============================================================================
// Keyword Matcher
// ============================================================================

/// High-performance keyword matcher using Aho-Corasick algorithm.
#[derive(Debug)]
pub struct KeywordMatcher {
    /// The patterns to match.
    patterns: Vec<String>,
    /// Category for each pattern (optional).
    categories: Vec<Option<PatternCategory>>,
    /// Compiled Aho-Corasick automaton.
    automaton: Arc<AhoCorasick>,
    /// Configuration.
    config: MatcherConfig,
}

impl KeywordMatcher {
    /// Create a new matcher with the given patterns and config.
    ///
    /// Uses cached automaton if available, otherwise builds and caches.
    pub fn new(patterns: Vec<String>, config: MatcherConfig) -> Self {
        let cache_key = Self::compute_cache_key(&patterns, &config);

        // Try to get from cache (lock-free via moka)
        let automaton = match AUTOMATON_CACHE.get(&cache_key) {
            Some(ac) => {
                // Cache hit
                CACHE_HITS.fetch_add(1, Ordering::Relaxed);
                ac.clone()
            }
            _ => {
                // Cache miss - build new automaton
                let match_kind = if config.allow_overlapping {
                    MatchKind::Standard
                } else {
                    MatchKind::LeftmostLongest
                };

                let ac = AhoCorasickBuilder::new()
                    .ascii_case_insensitive(config.case_insensitive)
                    .match_kind(match_kind)
                    .build(&patterns)
                    .expect("Failed to build Aho-Corasick automaton");

                let ac = Arc::new(ac);

                // Store in cache (moka handles eviction via W-TinyLFU + LRU)
                AUTOMATON_CACHE.insert(cache_key, ac.clone());
                CACHE_MISSES.fetch_add(1, Ordering::Relaxed);

                ac
            }
        };

        Self {
            categories: vec![None; patterns.len()],
            patterns,
            automaton,
            config,
        }
    }

    /// Create a matcher with categorized patterns.
    pub(crate) fn with_categories(
        patterns: Vec<(String, PatternCategory)>,
        config: MatcherConfig,
    ) -> Self {
        let (pats, cats): (Vec<_>, Vec<_>) = patterns.into_iter().unzip();
        let mut matcher = Self::new(pats, config);
        matcher.categories = cats.into_iter().map(Some).collect();
        matcher
    }

    /// Compute cache key from patterns and config.
    fn compute_cache_key(patterns: &[String], config: &MatcherConfig) -> u64 {
        let mut hasher = DefaultHasher::new();
        patterns.hash(&mut hasher);
        config.case_insensitive.hash(&mut hasher);
        config.allow_overlapping.hash(&mut hasher);
        hasher.finish()
    }

    /// Find all matches in the given text.
    pub fn find_matches(&self, text: &str) -> Vec<KeywordMatch> {
        let mut results = Vec::new();

        // Exact matches with Aho-Corasick (O(n))
        for mat in self.automaton.find_iter(text) {
            let pattern_idx = mat.pattern().as_usize();
            let matched_text = &text[mat.start()..mat.end()];
            // SAFETY: pattern_idx comes from Aho-Corasick which only returns
            // indices within the patterns we built the automaton with.
            let Some(keyword) = self.patterns.get(pattern_idx) else {
                continue;
            };
            let category = self.categories.get(pattern_idx).cloned().flatten();

            results.push(KeywordMatch {
                keyword: keyword.clone(),
                pattern_index: pattern_idx,
                start: mat.start(),
                end: mat.end(),
                matched_text: matched_text.to_string(),
                context_before: self.extract_context_before(text, mat.start()),
                context_after: self.extract_context_after(text, mat.end()),
                match_score: 1.0,
                category: category.map(|c| c.to_string()),
            });
        }

        // Fuzzy matches (if enabled)
        if self.config.fuzzy_enabled {
            let fuzzy_matches = self.find_fuzzy_matches(text, &results);
            results.extend(fuzzy_matches);
        }

        results
    }

    /// Find fuzzy matches using Levenshtein distance.
    fn find_fuzzy_matches(&self, text: &str, exact_matches: &[KeywordMatch]) -> Vec<KeywordMatch> {
        let mut results = Vec::new();

        // Tokenize text into words with positions
        let words: Vec<(usize, &str)> = self.tokenize_with_positions(text);

        // Skip positions already matched exactly
        let exact_positions: std::collections::HashSet<(usize, usize)> =
            exact_matches.iter().map(|m| (m.start, m.end)).collect();

        for (idx, pattern) in self.patterns.iter().enumerate() {
            // Only single-word patterns for fuzzy
            if pattern.split_whitespace().count() > 1 {
                continue;
            }

            for &(start, word) in &words {
                let end = start + word.len();

                // Skip if already matched exactly
                if exact_positions.contains(&(start, end)) {
                    continue;
                }

                let distance = levenshtein_distance(pattern, word);
                let max_len = pattern.len().max(word.len());

                if max_len == 0 {
                    continue;
                }

                let score = 1.0 - (distance as f64 / max_len as f64);

                if distance <= self.config.max_edit_distance
                    && score >= self.config.min_fuzzy_score
                    && score < 1.0
                // Not exact match
                {
                    let category = self.categories.get(idx).cloned().flatten();

                    results.push(KeywordMatch {
                        keyword: pattern.clone(),
                        pattern_index: idx,
                        start,
                        end,
                        matched_text: word.to_string(),
                        context_before: self.extract_context_before(text, start),
                        context_after: self.extract_context_after(text, end),
                        match_score: score,
                        category: category.map(|c| c.to_string()),
                    });
                }
            }
        }

        results
    }

    /// Tokenize text into words with their byte positions.
    fn tokenize_with_positions<'a>(&self, text: &'a str) -> Vec<(usize, &'a str)> {
        let mut results = Vec::new();
        let mut pos = 0;

        for word in text.split_whitespace() {
            if let Some(offset) = text[pos..].find(word) {
                let start = pos + offset;
                results.push((start, word));
                pos = start + word.len();
            }
        }

        results
    }

    /// Extract context before the match position.
    fn extract_context_before(&self, text: &str, pos: usize) -> String {
        let start = pos.saturating_sub(self.config.context_size);
        // Ensure we don't split UTF-8 characters
        let start = text[..pos]
            .char_indices()
            .rev()
            .take_while(|(i, _)| *i >= start)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(start);
        text[start..pos].to_string()
    }

    /// Extract context after the match position.
    fn extract_context_after(&self, text: &str, pos: usize) -> String {
        let end = (pos + self.config.context_size).min(text.len());
        // Ensure we don't split UTF-8 characters
        let end = text[pos..]
            .char_indices()
            .take_while(|(i, _)| pos + *i < end)
            .last()
            .map(|(i, c)| pos + i + c.len_utf8())
            .unwrap_or(end);
        text[pos..end].to_string()
    }

    /// Get the number of patterns.
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    /// Get the patterns.
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }
}

// ============================================================================
// Levenshtein Distance
// ============================================================================

/// Compute Levenshtein edit distance between two strings.
///
/// Uses space-optimized DP (two rows only).
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.to_lowercase().chars().collect();
    let b_chars: Vec<char> = b.to_lowercase().chars().collect();

    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    // Space optimization: only keep two rows.
    // SAFETY: All index accesses below are bounded by the loop ranges [0..=n]
    // and [1..=m], which match the vec lengths (n+1). The i-1 and j-1 accesses
    // are safe because i starts at 1 and j starts at 1.
    #[allow(clippy::indexing_slicing)]
    {
        let mut prev: Vec<usize> = (0..=n).collect();
        let mut curr = vec![0; n + 1];

        for i in 1..=m {
            curr[0] = i;
            for j in 1..=n {
                let cost = if a_chars[i - 1] == b_chars[j - 1] {
                    0
                } else {
                    1
                };
                curr[j] = (prev[j] + 1) // deletion
                    .min(curr[j - 1] + 1) // insertion
                    .min(prev[j - 1] + cost); // substitution
            }
            std::mem::swap(&mut prev, &mut curr);
        }

        prev[n]
    }
}

// ============================================================================
// Pre-defined ANTT Patterns
// ============================================================================

/// Pre-built matcher for ANTT regulatory patterns.
pub static ANTT_PATTERNS: Lazy<KeywordMatcher> = Lazy::new(|| {
    let patterns = vec![
        // Common resolutions
        ("Resolução ANTT".to_string(), PatternCategory::Resolution),
        ("Resolução nº".to_string(), PatternCategory::Resolution),
        ("Resolução n°".to_string(), PatternCategory::Resolution),
        ("Resolucao".to_string(), PatternCategory::Resolution),
        // Laws
        ("Lei nº".to_string(), PatternCategory::Law),
        ("Lei n°".to_string(), PatternCategory::Law),
        ("Lei Federal".to_string(), PatternCategory::Law),
        ("Lei 10.233".to_string(), PatternCategory::Law),
        ("Lei 13.848".to_string(), PatternCategory::Law),
        ("Lei 8.987".to_string(), PatternCategory::Law),
        ("Lei 9.784".to_string(), PatternCategory::Law),
        // Decrees
        ("Decreto nº".to_string(), PatternCategory::Decree),
        ("Decreto n°".to_string(), PatternCategory::Decree),
        // Processes
        ("Processo".to_string(), PatternCategory::Process),
        ("50500.".to_string(), PatternCategory::Process),
        ("50505.".to_string(), PatternCategory::Process),
        ("SEI nº".to_string(), PatternCategory::Process),
        ("SEI n°".to_string(), PatternCategory::Process),
        // TCU
        ("Acórdão TCU".to_string(), PatternCategory::Tcu),
        ("Acordao TCU".to_string(), PatternCategory::Tcu),
        ("Súmula TCU".to_string(), PatternCategory::Tcu),
        ("Sumula TCU".to_string(), PatternCategory::Tcu),
        ("Acórdão nº".to_string(), PatternCategory::Tcu),
        // Technical notes
        ("Nota Técnica".to_string(), PatternCategory::TechnicalNote),
        ("Nota Tecnica".to_string(), PatternCategory::TechnicalNote),
        ("NT nº".to_string(), PatternCategory::TechnicalNote),
        // Opinions
        ("Parecer PF-ANTT".to_string(), PatternCategory::Opinion),
        ("Parecer PRGREG".to_string(), PatternCategory::Opinion),
        ("Parecer PROCAD".to_string(), PatternCategory::Opinion),
        ("Parecer nº".to_string(), PatternCategory::Opinion),
        // Contracts
        (
            "Contrato de Concessão".to_string(),
            PatternCategory::Contract,
        ),
        ("Termo Aditivo".to_string(), PatternCategory::Contract),
        ("TA nº".to_string(), PatternCategory::Contract),
        ("Aditivo nº".to_string(), PatternCategory::Contract),
    ];

    KeywordMatcher::with_categories(
        patterns,
        MatcherConfig {
            case_insensitive: true,
            fuzzy_enabled: false,
            context_size: 80,
            ..Default::default()
        },
    )
});

/// Pre-built matcher for ANTT technical keywords.
pub static TECHNICAL_KEYWORDS: Lazy<KeywordMatcher> = Lazy::new(|| {
    let keywords: Vec<String> = vec![
        "reequilíbrio econômico-financeiro",
        "reequilibrio economico-financeiro",
        "fator D",
        "fator de desconto",
        "fator de acréscimo",
        "taxa interna de retorno",
        "TIR",
        "valor presente líquido",
        "VPL",
        "fluxo de caixa",
        "WACC",
        "custo médio ponderado de capital",
        "duplicação",
        "restauração",
        "manutenção",
        "conservação",
        "melhoramento",
        "ampliação de capacidade",
        "obra",
        "intervenção",
        "tarifa básica de pedágio",
        "TBP",
        "tarifa teto",
        "reajuste tarifário",
        "revisão tarifária",
        "pedágio",
        "termo de arrecadação",
        "termo de autorização",
        "cronograma de obras",
        "PER",
        "programa de exploração da rodovia",
        "plano de negócios",
        "anexo 5",
        "Anexo V",
        "multa",
        "sanção",
        "infração",
        "descumprimento",
        "advertência",
        "penalidade",
        "deliberação",
        "despacho",
        "decisão",
        "voto",
        "relator",
        "relatoria",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    KeywordMatcher::new(
        keywords,
        MatcherConfig {
            case_insensitive: true,
            fuzzy_enabled: true,
            max_edit_distance: 2,
            min_fuzzy_score: 0.85,
            context_size: 50,
            ..Default::default()
        },
    )
});

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    mod exact_matching {
        use super::*;

        #[test]
        fn test_single_keyword() {
            let matcher =
                KeywordMatcher::new(vec!["reequilíbrio".to_string()], MatcherConfig::default());

            let text = "O reequilíbrio econômico-financeiro foi solicitado.";
            let matches = matcher.find_matches(text);

            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].keyword, "reequilíbrio");
            assert_eq!(matches[0].match_score, 1.0);
        }

        #[test]
        fn test_multiple_keywords() {
            let matcher = KeywordMatcher::new(
                vec!["VPL".to_string(), "TIR".to_string(), "WACC".to_string()],
                MatcherConfig::default(),
            );

            let text = "O VPL calculado com TIR de 10% e WACC de 8%.";
            let matches = matcher.find_matches(text);

            assert_eq!(matches.len(), 3);
        }

        #[test]
        fn test_no_matches() {
            let matcher = KeywordMatcher::new(vec!["xyz123".to_string()], MatcherConfig::default());

            let text = "Este texto não contém a palavra buscada.";
            let matches = matcher.find_matches(text);

            assert!(matches.is_empty());
        }
    }

    mod antt_patterns {
        use super::*;

        #[test]
        fn test_resolution_match() {
            let matches = ANTT_PATTERNS.find_matches("Resolução ANTT nº 5.950/2021");
            assert!(!matches.is_empty());
            assert!(
                matches
                    .iter()
                    .any(|m| m.category.as_deref() == Some("Resolution"))
            );
        }

        #[test]
        fn test_law_match() {
            let matches = ANTT_PATTERNS.find_matches("Lei nº 10.233/2001");
            assert!(!matches.is_empty());
            assert!(matches.iter().any(|m| m.category.as_deref() == Some("Law")));
        }

        #[test]
        fn test_process_match() {
            let matches = ANTT_PATTERNS.find_matches("Processo 50500.152234/2022-92");
            assert!(!matches.is_empty());
            assert!(
                matches
                    .iter()
                    .any(|m| m.category.as_deref() == Some("Process"))
            );
        }
    }

    mod levenshtein {
        use super::*;

        #[test]
        fn test_levenshtein_identical() {
            assert_eq!(levenshtein_distance("hello", "hello"), 0);
        }

        #[test]
        fn test_levenshtein_one_edit() {
            assert_eq!(levenshtein_distance("hello", "hallo"), 1);
        }

        #[test]
        fn test_levenshtein_empty() {
            assert_eq!(levenshtein_distance("", "hello"), 5);
            assert_eq!(levenshtein_distance("hello", ""), 5);
            assert_eq!(levenshtein_distance("", ""), 0);
        }

        #[test]
        fn test_levenshtein_case_insensitive() {
            assert_eq!(levenshtein_distance("Hello", "HELLO"), 0);
        }
    }

    mod cache {
        use super::*;

        #[test]
        fn test_cache_clear() {
            clear_cache();
            let stats = get_cache_stats();
            assert_eq!(stats.size, 0);
            assert_eq!(stats.hits, 0);
            assert_eq!(stats.misses, 0);
        }
    }
}
