//! PII Scanner — High-performance PII detection with whitelist support.
//!
//! Detects Brazilian PII patterns (CPF, CNPJ, RG, CNH, SUS, SEI Process, Email, Phone)
//! and filters against 13 whitelist patterns.

use std::collections::HashSet;

use regex::{Regex, RegexSet};
use sha2::{Digest, Sha256};

/// PII pattern definitions: (name, regex, severity).
/// The email pattern matches ALL emails; `@antt.gov.br` is filtered in code
/// (Rust `regex` crate does not support negative lookahead).
const PII_PATTERNS: &[(&str, &str, &str)] = &[
    ("cpf", r"(?i)\b\d{3}\.\d{3}\.\d{3}-\d{2}\b", "high"),
    ("cnpj", r"(?i)\b\d{2}\.\d{3}\.\d{3}/\d{4}-\d{2}\b", "high"),
    ("rg", r"(?i)\b\d{2}\.\d{3}\.\d{3}-[\dXx]\b", "high"),
    ("cnh", r"(?i)\b\d{11}\b", "high"),
    ("sus", r"(?i)\b\d{3}\.\d{4}\.\d{4}-\d{2}\b", "high"),
    (
        "processo_sei",
        r"(?i)\b50500\.\d{6}/\d{4}-\d{2}\b",
        "medium",
    ),
    (
        "email_pessoal",
        r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b",
        "medium",
    ),
    (
        "telefone_br",
        r"(?i)\b\(?\d{2}\)?\s?\d{4,5}-?\d{4}\b",
        "low",
    ),
];

/// Index of the email pattern in PII_PATTERNS (for domain filtering).
const EMAIL_PATTERN_IDX: usize = 6;

/// Whitelist patterns — checked per-finding within a context window (±40 chars).
/// Matches Python's `ANTTPIIDetectorConfig.whitelist_patterns` exactly.
const WHITELIST_PATTERNS: &[&str] = &[
    r"(?i)process\.env\.",
    r"(?i)os\.environ",
    r"(?i)getenv\(",
    r"(?i)\$\{.*\}",
    r"(?i)<.*>",
    r"(?i)example",
    r"(?i)test",
    r"(?i)mock",
    r"(?i)fake",
    r"(?i)placeholder",
    r"(?i)xxx+",
    r"(?i)000\.000\.000-00",
    r"(?i)00\.000\.000/0000-00",
];

/// Comment line prefixes to skip (matches Python's `_scan_file` logic).
const COMMENT_PREFIXES: &[&str] = &["#", "//", "/*", "*", "<!--"];

/// Check if a line is a comment (should be skipped during PII scanning).
fn is_comment_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    COMMENT_PREFIXES
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

/// Check if an email address belongs to `antt.gov.br` (institutional, not PII).
fn is_antt_email(email: &str) -> bool {
    let lower = email.to_lowercase();
    if let Some(at_pos) = lower.find('@') {
        let domain = &lower[at_pos + 1..];
        domain == "antt.gov.br"
    } else {
        false
    }
}

/// Count unique characters in a string.
/// Matches Python's `len(set(match_text)) < 4` whitelist check.
fn unique_char_count(s: &str) -> usize {
    s.chars().collect::<HashSet<_>>().len()
}

/// Extract a UTF-8-safe context window around a match within a line.
///
/// Returns the slice `line[start-radius..end+radius]`, clamped to line bounds
/// and adjusted to valid UTF-8 char boundaries.
fn extract_context_window(line: &str, match_start: usize, match_end: usize, radius: usize) -> &str {
    let mut ctx_start = match_start.saturating_sub(radius);
    let mut ctx_end = (match_end + radius).min(line.len());
    // Walk to valid UTF-8 char boundaries (stable since Rust 1.9)
    while ctx_start > 0 && !line.is_char_boundary(ctx_start) {
        ctx_start -= 1;
    }
    while ctx_end < line.len() && !line.is_char_boundary(ctx_end) {
        ctx_end += 1;
    }
    &line[ctx_start..ctx_end]
}

/// PII type enumeration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum PIIType {
    /// CPF - Brazilian individual taxpayer ID.
    CPF,
    /// CNPJ - Brazilian corporate taxpayer ID.
    CNPJ,
    /// RG - Registro Geral (Brazilian identity card).
    RG,
    /// CNH - Carteira Nacional de Habilitacao (driver's license).
    CNH,
    /// SUS - Sistema Unico de Saude card number.
    SUS,
    /// SEI Process number.
    ProcessoSEI,
    /// Personal email address.
    EmailPessoal,
    /// Brazilian phone number.
    TelefoneBR,
}

impl PIIType {
    /// From pattern name string.
    pub fn from_pattern(name: &str) -> Option<Self> {
        match name {
            "cpf" => Some(Self::CPF),
            "cnpj" => Some(Self::CNPJ),
            "rg" => Some(Self::RG),
            "cnh" => Some(Self::CNH),
            "sus" => Some(Self::SUS),
            "processo_sei" => Some(Self::ProcessoSEI),
            "email_pessoal" => Some(Self::EmailPessoal),
            "telefone_br" => Some(Self::TelefoneBR),
            _ => None,
        }
    }

    /// Get severity level.
    pub fn severity(&self) -> &'static str {
        match self {
            Self::CPF | Self::CNPJ | Self::RG | Self::CNH | Self::SUS => "high",
            Self::ProcessoSEI | Self::EmailPessoal => "medium",
            Self::TelefoneBR => "low",
        }
    }
}

/// A single PII detection finding.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PIIFinding {
    /// Pattern type: "cpf", "cnpj", "rg", "cnh", "sus", "processo_sei", "email_pessoal", "telefone_br".
    pub pattern_name: String,
    /// 1-based line number in the content.
    pub line_number: usize,
    /// Matched text (truncated to 50 chars).
    pub matched_text: String,
    /// Byte offset within the line.
    pub column: usize,
    /// Severity: "high", "medium", "low".
    pub severity: String,
}

/// High-performance PII scanner with whitelist support.
///
/// Uses `RegexSet` for quick per-line PII type detection, then individual
/// `Regex` for extracting match positions only when needed.
#[derive(Debug)]
pub struct PIIScanner {
    /// RegexSet with all 8 PII patterns for O(n) line scanning.
    pii_set: RegexSet,
    /// Individual Regex per PII type for extracting match positions.
    individual: Vec<Regex>,
    /// RegexSet with 13 whitelist patterns.
    whitelist_set: RegexSet,
    /// Severity per PII pattern index.
    severities: Vec<&'static str>,
    /// Pattern names per index.
    names: Vec<&'static str>,
}

impl Default for PIIScanner {
    fn default() -> Self {
        Self::build()
    }
}

impl PIIScanner {
    /// Build a new scanner with all PII and whitelist patterns compiled.
    pub fn build() -> Self {
        let pii_patterns: Vec<&str> = PII_PATTERNS.iter().map(|(_, p, _)| *p).collect();
        let pii_set = RegexSet::new(&pii_patterns).expect("All PII patterns must compile");

        let individual: Vec<Regex> = pii_patterns
            .iter()
            .map(|p| Regex::new(p).expect("Individual PII regex must compile"))
            .collect();

        let whitelist_set =
            RegexSet::new(WHITELIST_PATTERNS).expect("All whitelist patterns must compile");

        let severities: Vec<&str> = PII_PATTERNS.iter().map(|(_, _, s)| *s).collect();
        let names: Vec<&str> = PII_PATTERNS.iter().map(|(n, _, _)| *n).collect();

        Self {
            pii_set,
            individual,
            whitelist_set,
            severities,
            names,
        }
    }

    /// Create new scanner (alias for build).
    pub fn new() -> Self {
        Self::build()
    }

    /// Scan content line-by-line for PII, applying per-finding whitelist and comment filtering.
    ///
    /// Whitelist is checked per-finding within a ±40 char context window around
    /// each match, NOT at the line level. This prevents false negatives where
    /// a whitelist word (e.g., "test") elsewhere on the line would suppress
    /// legitimate PII findings.
    pub fn scan_text(&self, content: &str) -> Vec<PIIFinding> {
        let mut findings = Vec::new();

        for (line_idx, line) in content.lines().enumerate() {
            let line_number = line_idx + 1;

            // Skip comment lines (parity with Python _scan_file)
            if is_comment_line(line) {
                continue;
            }

            // Quick check: which PII types are present on this line?
            let pii_matches: Vec<usize> = self.pii_set.matches(line).iter().collect();
            if pii_matches.is_empty() {
                continue;
            }

            // Extract positions only for matched PII types
            // SAFETY: idx comes from pii_set.matches() which returns indices
            // within 0..pii_set.len(). The individual, names, and severities
            // vectors are all built from the same PII_PATTERNS source with
            // identical length, so idx is always in bounds.
            #[allow(clippy::indexing_slicing)]
            for &idx in &pii_matches {
                for m in self.individual[idx].find_iter(line) {
                    let matched_text = m.as_str();

                    // Email domain filtering (replaces Python negative lookahead)
                    if idx == EMAIL_PATTERN_IDX && is_antt_email(matched_text) {
                        continue;
                    }

                    // Low unique char count = placeholder (e.g., "000.000.000-00")
                    if unique_char_count(matched_text) < 4 {
                        continue;
                    }

                    // Per-finding whitelist: check ±40 char context window around match.
                    // This fixes BUG-1 where line-level whitelist suppressed ALL findings
                    // when ANY whitelist word appeared anywhere on the line.
                    let context_window = extract_context_window(line, m.start(), m.end(), 40);
                    if self.whitelist_set.is_match(context_window) {
                        continue;
                    }

                    // Truncate matched text to 50 chars (parity with Python)
                    let text: String = matched_text.chars().take(50).collect();

                    findings.push(PIIFinding {
                        pattern_name: self.names[idx].to_string(),
                        line_number,
                        matched_text: text,
                        column: m.start(),
                        severity: self.severities[idx].to_string(),
                    });
                }
            }
        }

        findings
    }

    /// Quick check: does content contain ANY PII?
    ///
    /// Uses per-finding whitelist (same as `scan_text`) to avoid false negatives.
    pub(crate) fn check_pii(&self, content: &str) -> bool {
        for line in content.lines() {
            if is_comment_line(line) {
                continue;
            }
            let pii_matches: Vec<usize> = self.pii_set.matches(line).iter().collect();
            // SAFETY: idx comes from pii_set.matches() — always in bounds
            // for individual vec (same source length as pii_set).
            #[allow(clippy::indexing_slicing)]
            for &idx in &pii_matches {
                for m in self.individual[idx].find_iter(line) {
                    let text = m.as_str();
                    // Email domain filtering
                    if idx == EMAIL_PATTERN_IDX && is_antt_email(text) {
                        continue;
                    }
                    // Placeholder filtering
                    if unique_char_count(text) < 4 {
                        continue;
                    }
                    // Per-finding whitelist check
                    let ctx = extract_context_window(line, m.start(), m.end(), 40);
                    if self.whitelist_set.is_match(ctx) {
                        continue;
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Compute SHA-256 hash of content (for cache keys).
    pub(crate) fn hash_content(&self, content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Scan content for PII, returning all findings.
    ///
    /// Skips comment lines and whitelisted lines.
    /// Filters institutional emails (`@antt.gov.br`).
    pub fn scan_content(&self, content: &str) -> Vec<PIIFinding> {
        self.scan_text(content)
    }

    /// Quick boolean check: does content contain any PII?
    pub fn has_pii(&self, content: &str) -> bool {
        self.check_pii(content)
    }

    /// SHA-256 hash of content for cache key generation.
    pub fn content_hash(&self, content: &str) -> String {
        self.hash_content(content)
    }

    /// Return the number of PII patterns.
    pub fn pii_pattern_count(&self) -> usize {
        PII_PATTERNS.len()
    }

    /// Return the number of whitelist patterns.
    pub fn whitelist_pattern_count(&self) -> usize {
        WHITELIST_PATTERNS.len()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    fn scanner() -> PIIScanner {
        PIIScanner::build()
    }

    // ── CPF ────────────────────────────────────────────────────────────

    #[test]
    fn test_cpf_valid() {
        let findings = scanner().scan_text("CPF: 123.456.789-00");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern_name, "cpf");
        assert_eq!(findings[0].severity, "high");
        assert_eq!(findings[0].matched_text, "123.456.789-00");
        assert_eq!(findings[0].line_number, 1);
    }

    #[test]
    fn test_cpf_placeholder_whitelisted() {
        // "000.000.000-00" has only 3 unique chars (0, ., -) → whitelisted
        let findings = scanner().scan_text("CPF: 000.000.000-00");
        assert!(findings.is_empty(), "Placeholder CPF should be whitelisted");
    }

    #[test]
    fn test_cpf_in_env_context() {
        let findings = scanner().scan_text("process.env.CPF = '123.456.789-00'");
        assert!(findings.is_empty(), "process.env context should whitelist");
    }

    // ── CNPJ ───────────────────────────────────────────────────────────

    #[test]
    fn test_cnpj_valid() {
        let findings = scanner().scan_text("CNPJ: 12.345.678/0001-90");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern_name, "cnpj");
        assert_eq!(findings[0].severity, "high");
    }

    #[test]
    fn test_cnpj_placeholder_whitelisted() {
        let findings = scanner().scan_text("CNPJ: 00.000.000/0000-00");
        assert!(
            findings.is_empty(),
            "Placeholder CNPJ should be whitelisted by whitelist pattern"
        );
    }

    // ── RG ─────────────────────────────────────────────────────────────

    #[test]
    fn test_rg_valid() {
        let findings = scanner().scan_text("RG: 12.345.678-9");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern_name, "rg");
        assert_eq!(findings[0].severity, "high");
        assert_eq!(findings[0].matched_text, "12.345.678-9");
    }

    #[test]
    fn test_rg_with_x_check_digit() {
        let findings = scanner().scan_text("RG: 12.345.678-X");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern_name, "rg");
        assert_eq!(findings[0].matched_text, "12.345.678-X");
    }

    // ── CNH ────────────────────────────────────────────────────────────

    #[test]
    fn test_cnh_valid() {
        let findings = scanner().scan_text("CNH: 12345678901");
        // Note: 11-digit numbers may match as both CNH and telefone_br due to pattern overlap
        // This is a known limitation - CNH is high severity, phone is low
        let cnh_count = findings.iter().filter(|f| f.pattern_name == "cnh").count();
        assert!(cnh_count >= 1, "Should find CNH pattern");
        assert_eq!(findings[0].severity, "high");
    }

    // ── SUS ────────────────────────────────────────────────────────────

    #[test]
    fn test_sus_valid() {
        let findings = scanner().scan_text("SUS: 123.4567.8901-23");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern_name, "sus");
        assert_eq!(findings[0].severity, "high");
    }

    // ── Processo SEI ───────────────────────────────────────────────────

    #[test]
    fn test_processo_sei() {
        let findings = scanner().scan_text("processo: 50500.123456/2024-01");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern_name, "processo_sei");
        assert_eq!(findings[0].severity, "medium");
    }

    // ── Email ──────────────────────────────────────────────────────────

    #[test]
    fn test_email_personal() {
        let findings = scanner().scan_text("contato: user@gmail.com");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern_name, "email_pessoal");
        assert_eq!(findings[0].severity, "medium");
    }

    #[test]
    fn test_email_institutional_not_detected() {
        let findings = scanner().scan_text("email: servidor@antt.gov.br");
        assert!(
            findings.is_empty(),
            "Institutional @antt.gov.br should NOT be detected"
        );
    }

    #[test]
    fn test_email_case_insensitive() {
        let findings = scanner().scan_text("EMAIL: User@Gmail.COM");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern_name, "email_pessoal");
    }

    #[test]
    fn test_email_antt_case_insensitive() {
        let findings = scanner().scan_text("email: USER@ANTT.GOV.BR");
        assert!(
            findings.is_empty(),
            "Institutional @ANTT.GOV.BR (uppercase) should NOT be detected"
        );
    }

    // ── Phone ──────────────────────────────────────────────────────────

    #[test]
    fn test_phone_with_area_code() {
        let findings = scanner().scan_text("telefone: (61) 99999-1234");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern_name, "telefone_br");
        assert_eq!(findings[0].severity, "low");
    }

    #[test]
    fn test_phone_without_parens() {
        let findings = scanner().scan_text("tel: 61 99999-1234");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern_name, "telefone_br");
    }

    // ── Whitelist ──────────────────────────────────────────────────────

    #[test]
    fn test_whitelist_process_env() {
        let findings = scanner().scan_text("process.env.CPF_KEY = '123.456.789-00'");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_whitelist_os_environ() {
        let findings = scanner().scan_text("os.environ['CPF'] = '123.456.789-00'");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_whitelist_getenv() {
        let findings = scanner().scan_text("getenv('CPF') returns 123.456.789-00");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_whitelist_example() {
        let findings = scanner().scan_text("example: 123.456.789-00");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_whitelist_test() {
        let findings = scanner().scan_text("test CPF: 123.456.789-00");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_whitelist_mock() {
        let findings = scanner().scan_text("mock data: 123.456.789-00");
        assert!(findings.is_empty());
    }

    // ── No PII ─────────────────────────────────────────────────────────

    #[test]
    fn test_no_pii() {
        let findings = scanner().scan_text("texto sem PII nenhum aqui");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_empty_content() {
        let findings = scanner().scan_text("");
        assert!(findings.is_empty());
    }

    // ── Multi-line / Multi-match ───────────────────────────────────────

    #[test]
    fn test_multiple_pii_same_line() {
        let findings = scanner().scan_text("CPF: 123.456.789-00, CNPJ: 12.345.678/0001-90");
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn test_multiline() {
        let content =
            "Linha 1: ok\nLinha 2: CPF 123.456.789-00\nLinha 3: ok\nLinha 4: (61) 99999-1234";
        let findings = scanner().scan_text(content);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].line_number, 2);
        assert_eq!(findings[1].line_number, 4);
    }

    // ── Comment Lines ──────────────────────────────────────────────────

    #[test]
    fn test_comment_hash_skipped() {
        let findings = scanner().scan_text("# CPF: 123.456.789-00");
        assert!(findings.is_empty(), "Comment lines should be skipped");
    }

    #[test]
    fn test_comment_double_slash_skipped() {
        let findings = scanner().scan_text("// CPF: 123.456.789-00");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_comment_html_skipped() {
        let findings = scanner().scan_text("<!-- CPF: 123.456.789-00 -->");
        assert!(findings.is_empty());
    }

    // ── has_pii ────────────────────────────────────────────────────────

    #[test]
    fn test_has_pii_true() {
        assert!(scanner().check_pii("CPF: 123.456.789-00"));
    }

    #[test]
    fn test_has_pii_false() {
        assert!(!scanner().check_pii("texto limpo sem dados pessoais"));
    }

    #[test]
    fn test_has_pii_whitelisted_false() {
        assert!(!scanner().check_pii("test CPF: 123.456.789-00"));
    }

    #[test]
    fn test_has_pii_antt_email_false() {
        assert!(!scanner().check_pii("email: user@antt.gov.br"));
    }

    // ── content_hash ───────────────────────────────────────────────────

    #[test]
    fn test_content_hash_deterministic() {
        let s = scanner();
        let h1 = s.hash_content("hello world");
        let h2 = s.hash_content("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_content_hash_different() {
        let s = scanner();
        let h1 = s.hash_content("hello");
        let h2 = s.hash_content("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_content_hash_format() {
        let h = scanner().hash_content("test");
        // SHA-256 produces 64 hex characters
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ── Pattern counts ─────────────────────────────────────────────────

    #[test]
    fn test_pii_pattern_count() {
        assert_eq!(PII_PATTERNS.len(), 8);
    }

    #[test]
    fn test_whitelist_pattern_count() {
        assert_eq!(WHITELIST_PATTERNS.len(), 13);
    }

    // ── Column tracking ────────────────────────────────────────────────

    #[test]
    fn test_column_tracking() {
        let findings = scanner().scan_text("prefix 123.456.789-00 suffix");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].column, 7); // "prefix " = 7 bytes
    }

    // ── Edge cases ─────────────────────────────────────────────────────

    #[test]
    fn test_matched_text_truncated() {
        // Create a line with a very long potential match (phone-like)
        let content = "text 123.456.789-12";
        let findings = scanner().scan_text(content);
        assert!(!findings.is_empty());
        assert!(findings[0].matched_text.len() <= 50);
    }

    // ── PIIType tests ──────────────────────────────────────────────────

    #[test]
    fn test_pii_type_from_pattern() {
        assert!(matches!(PIIType::from_pattern("cpf"), Some(PIIType::CPF)));
        assert!(matches!(PIIType::from_pattern("cnpj"), Some(PIIType::CNPJ)));
        assert!(matches!(PIIType::from_pattern("rg"), Some(PIIType::RG)));
        assert!(matches!(PIIType::from_pattern("cnh"), Some(PIIType::CNH)));
        assert!(matches!(PIIType::from_pattern("sus"), Some(PIIType::SUS)));
        assert!(matches!(
            PIIType::from_pattern("processo_sei"),
            Some(PIIType::ProcessoSEI)
        ));
        assert!(matches!(
            PIIType::from_pattern("email_pessoal"),
            Some(PIIType::EmailPessoal)
        ));
        assert!(matches!(
            PIIType::from_pattern("telefone_br"),
            Some(PIIType::TelefoneBR)
        ));
        assert!(PIIType::from_pattern("unknown").is_none());
    }

    #[test]
    fn test_pii_type_severity() {
        assert_eq!(PIIType::CPF.severity(), "high");
        assert_eq!(PIIType::CNPJ.severity(), "high");
        assert_eq!(PIIType::RG.severity(), "high");
        assert_eq!(PIIType::CNH.severity(), "high");
        assert_eq!(PIIType::SUS.severity(), "high");
        assert_eq!(PIIType::ProcessoSEI.severity(), "medium");
        assert_eq!(PIIType::EmailPessoal.severity(), "medium");
        assert_eq!(PIIType::TelefoneBR.severity(), "low");
    }
}
