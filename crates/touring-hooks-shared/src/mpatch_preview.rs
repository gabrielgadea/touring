//! `mpatch_preview` — fuzzy patch preview via mpatch library
//!
//! Provides dry-run fuzzy patching that can detect what would change
//! before applying. Used by pre_write hook and plan_commit pipeline.

/// Dry-run result of a fuzzy patch attempt: whether it matched and what it would produce.
#[derive(Debug, Clone)]
pub struct PatchPreview {
    /// Whether the patch could be located and applied in dry-run.
    pub matched: bool,
    /// The matching strategy that succeeded.
    pub method: PatchMethod,
    /// Match confidence in `[0.0, 1.0]` reported by the apply engine.
    pub confidence: f32,
    /// The proposed post-patch content (dry-run only; not written to disk).
    pub preview: String,
}

/// Strategy used to locate the patch target, from strictest to most lenient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchMethod {
    /// Byte-for-byte exact match of the context.
    Exact,
    /// Match ignoring leading/trailing whitespace differences.
    Whitespace,
    /// Approximate match via fuzzy similarity scoring.
    Fuzzy,
}

/// Feature-gated impl — only available when `mpatch-fuzzy` is enabled.
#[cfg(feature = "mpatch-fuzzy")]
impl PatchPreview {
    /// Compute a dry-run patch preview without applying.
    ///
    /// Parses the diff content, then applies it in dry-run mode to get
    /// the proposed result and confidence metrics from the apply report.
    pub fn dry_run(source: &str, diff: &str) -> Option<Self> {
        use mpatch::{ApplyOptions, parse_auto, try_apply_patch_to_content};

        // Parse the diff content to extract patches.
        let patches = parse_auto(diff).ok()?;
        if patches.len() != 1 {
            return None;
        }
        let patch = patches.into_iter().next()?;

        // Apply in dry-run mode with fuzz factor to get confidence.
        let options = ApplyOptions {
            dry_run: true,
            fuzz_factor: 0.7,
        };
        let result = try_apply_patch_to_content(&patch, Some(source), &options).ok()?;

        let report = &result.report;
        let hunk_results = &report.hunk_results;

        // Calculate success metrics from hunk results.
        let total = hunk_results.len();
        let failures = hunk_results
            .iter()
            .filter(|r| matches!(r, mpatch::HunkApplyStatus::Failed(_)))
            .count();

        let (method, confidence) = if failures == 0 && total > 0 {
            // Check if any fuzzy matching was used by examining match_type.
            let has_fuzzy = hunk_results.iter().any(|r| {
                if let mpatch::HunkApplyStatus::Applied { match_type, .. } = r {
                    matches!(match_type, mpatch::MatchType::Fuzzy { .. })
                } else {
                    false
                }
            });
            if has_fuzzy {
                (PatchMethod::Fuzzy, 0.85) // Fuzzy used but all succeeded
            } else {
                (PatchMethod::Exact, 1.0)
            }
        } else {
            let success_rate = if total > 0 {
                (total - failures) as f32 / total as f32
            } else {
                0.0
            };
            (PatchMethod::Fuzzy, success_rate)
        };

        Some(PatchPreview {
            matched: true,
            method,
            confidence,
            preview: result.new_content,
        })
    }

    /// Returns true if patch can be applied with confidence >= threshold.
    pub fn is_confident(&self, threshold: f32) -> bool {
        self.matched && self.confidence >= threshold
    }
}

/// Stub impl — always available when feature is off.
#[cfg(not(feature = "mpatch-fuzzy"))]
impl PatchPreview {
    /// Stub: dry_run is not available without mpatch.
    pub fn dry_run(_source: &str, _diff: &str) -> Option<Self> {
        None
    }

    /// Stub: always returns false since mpatch is not available.
    pub fn is_confident(&self, _threshold: f32) -> bool {
        false
    }
}

/// Feature-gated: only available when `mpatch-fuzzy` feature is enabled.
#[cfg(feature = "mpatch-fuzzy")]
pub fn preview_patch(source: &str, diff: &str) -> Option<PatchPreview> {
    PatchPreview::dry_run(source, diff)
}

/// Stub when feature is off — always returns None.
#[cfg(not(feature = "mpatch-fuzzy"))]
pub fn preview_patch(_source: &str, _diff: &str) -> Option<PatchPreview> {
    None
}

/// Testes do motor de preview. O módulo tinha **zero** cobertura até 03/08/2026:
/// seu comportamento era garantido apenas por 3 asserções em
/// `cli_handlers_e2e.rs`, e essas rodam só sob `--all-features` — o perfil em que
/// a feature `mpatch-fuzzy` chegava ao CHAMADOR mas não a este motor, deixando
/// `preview_patch` no stub `None`. Com o motor sem testes próprios, nada apontava
/// para cá; o sintoma aparecia como "o patch não casa".
#[cfg(all(test, feature = "mpatch-fuzzy"))]
mod tests {
    use super::*;

    /// Dialetos de unified diff que o motor precisa aceitar. Fixa o contrato
    /// contra o parser do `mpatch` (1.6.4 no lock, `1.4.1` declarado no
    /// Cargo.toml — um bump minor silencioso passaria despercebido sem isto).
    #[test]
    fn accepts_the_unified_diff_dialects_the_callers_emit() {
        let cases: &[(&str, &str, &str)] = &[
            (
                "contexto sem o espaço inicial (tolerado)",
                "line one\nline two\nline three\n",
                "--- s.txt\n+++ s.txt\n@@ -1,3 +1,3 @@\nline one\n-line two\n+line two\nline three\n",
            ),
            (
                "contexto com o espaço inicial (canônico POSIX)",
                "line one\nline two\nline three\n",
                "--- s.txt\n+++ s.txt\n@@ -1,3 +1,3 @@\n line one\n-line two\n+line two\n line three\n",
            ),
            (
                "range abreviado @@ -1 +1 @@",
                "Hello world\n",
                "--- s.txt\n+++ s.txt\n@@ -1 +1 @@\n-Hello world\n+Hello world!\n",
            ),
            (
                "range explícito @@ -1,1 +1,1 @@",
                "Hello world\n",
                "--- s.txt\n+++ s.txt\n@@ -1,1 +1,1 @@\n-Hello world\n+Hello world!\n",
            ),
        ];
        for (name, source, diff) in cases {
            let preview = preview_patch(source, diff)
                .unwrap_or_else(|| panic!("dialeto rejeitado pelo motor: {name}"));
            assert!(preview.matched, "{name}: deveria casar");
            assert_eq!(preview.method, PatchMethod::Exact, "{name}: casamento exato");
            assert!(
                preview.confidence >= 0.99,
                "{name}: confiança {}",
                preview.confidence
            );
        }
    }

    #[test]
    fn applies_the_replacement_in_the_preview_text() {
        let preview = preview_patch(
            "Hello world\n",
            "--- s.txt\n+++ s.txt\n@@ -1 +1 @@\n-Hello world\n+Hello world!\n",
        )
        .expect("patch válido");
        assert!(
            preview.preview.contains("Hello world!"),
            "o preview carrega o texto já modificado, veio: {:?}",
            preview.preview
        );
    }

    #[test]
    fn rejects_input_that_is_not_a_diff() {
        assert!(
            preview_patch("qualquer coisa\n", "isto não é um diff").is_none(),
            "entrada não-diff tem de virar None, não um casamento falso"
        );
    }

    #[test]
    fn is_confident_respects_the_threshold() {
        let preview = preview_patch(
            "Hello world\n",
            "--- s.txt\n+++ s.txt\n@@ -1 +1 @@\n-Hello world\n+Hello world!\n",
        )
        .expect("patch válido");
        assert!(preview.is_confident(0.9));
        assert!(!preview.is_confident(1.01));
    }
}
