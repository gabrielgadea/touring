//! E2E tests for Tantivy signal integrations in touring-hooks.
//!
//! Tests verify all 5 Tantivy-powered signal functions are wired correctly
//! and produce valid output without crashing.

#![allow(clippy::all)]

#[cfg(test)]
mod tests {

    use touring_hooks::shared::signals;

    /// A project of its own (a `.git` marker keeps the root from normalizing to
    /// `$HOME`). Every signal used to be called with `None` — the user's REAL
    /// global index — and asserted `is_none() || is_some()`, which nothing can
    /// fail (cross-audit 14/09/2026, D9).
    fn project() -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        tmp
    }

    // ── Signal function smoke tests ─────────────────────────────────────────────

    /// Verify `tantivy_related_docs_signal` returns Some or None without panicking.
    #[test]
    fn tantivy_related_docs_signal_never_panics() {
        let root = project();
        let result =
            signals::tantivy_related_docs_signal(Some(root.path()), "src/shared/signals.rs");
        assert!(
            result.is_none(),
            "an empty project index has nothing to relate: {result:?}"
        );
    }

    /// Verify `tantivy_fuzzy_file_signal` returns Some or None without panicking.
    #[test]
    fn tantivy_fuzzy_file_signal_never_panics() {
        let root = project();
        let result = signals::tantivy_fuzzy_file_signal(Some(root.path()), "src/shared/signals.rs");
        assert!(
            result.is_none(),
            "an empty project index has nothing to relate: {result:?}"
        );
    }

    /// Verify `tantivy_kind_context_signal` returns Some or None without panicking.
    #[test]
    fn tantivy_kind_context_signal_never_panics() {
        let root = project();
        let result =
            signals::tantivy_kind_context_signal(Some(root.path()), "src/shared/signals.rs");
        assert!(
            result.is_none(),
            "an empty project index has nothing to relate: {result:?}"
        );
    }

    /// Verify `tantivy_crate_origin_signal` returns Some or None without panicking.
    #[test]
    fn tantivy_crate_origin_signal_never_panics() {
        let root = project();
        let result =
            signals::tantivy_crate_origin_signal(Some(root.path()), "src/shared/signals.rs");
        assert!(
            result.is_none(),
            "an empty project index has nothing to relate: {result:?}"
        );
    }

    /// Verify `tantivy_fuzzy_symbol_signal` returns Some or None without panicking.
    #[test]
    fn tantivy_fuzzy_symbol_signal_never_panics() {
        let root = project();
        let result =
            signals::tantivy_fuzzy_symbol_signal(Some(root.path()), "src/shared/signals.rs");
        assert!(
            result.is_none(),
            "an empty project index has nothing to relate: {result:?}"
        );
    }

    // ── Signal weight validation ────────────────────────────────────────────────

    /// All Tantivy signals must have weight in [0.0, 1.0].
    #[test]
    fn all_tantivy_signals_have_valid_weight() {
        let root = project();
        let root = Some(root.path());
        let cases = [
            signals::tantivy_related_docs_signal(root, "src/shared/signals.rs"),
            signals::tantivy_fuzzy_file_signal(root, "src/shared/signals.rs"),
            signals::tantivy_kind_context_signal(root, "src/shared/signals.rs"),
            signals::tantivy_crate_origin_signal(root, "src/shared/signals.rs"),
            signals::tantivy_fuzzy_symbol_signal(root, "src/shared/signals.rs"),
        ];

        for case in cases {
            if let Some((weight, label)) = case {
                assert!(
                    weight >= 0.0 && weight <= 1.0,
                    "weight {weight} out of range for {label}"
                );
                assert!(!label.is_empty(), "label must be non-empty");
            }
        }
    }

    // ── Edge case: short paths ───────────────────────────────────────────────────

    /// Short paths (< 3 chars) must not cause panics.
    #[test]
    fn short_path_does_not_panic() {
        let root = project();
        let result = signals::tantivy_fuzzy_file_signal(Some(root.path()), "a.rs");
        assert!(result.is_none()); // < 3 chars, fuzzy disabled
    }

    /// Empty path must not cause panic.
    #[test]
    fn empty_path_does_not_panic() {
        let root = project();
        let result = signals::tantivy_fuzzy_file_signal(Some(root.path()), "");
        assert!(result.is_none());
    }

    // ── FastMetadata enrichment tests ───────────────────────────────────────────

    #[test]
    fn fast_metadata_with_language_never_panics() {
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let meta = touring_hooks::shared::metadata_collector::FastMetadata::from_path(&file_path)
            .unwrap()
            .with_language();

        assert!(meta.language.is_some());
    }

    #[test]
    fn fast_metadata_with_feature_flags_never_panics() {
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let meta = touring_hooks::shared::metadata_collector::FastMetadata::from_path(&file_path)
            .unwrap()
            .with_feature_flags();

        // No Cargo.toml above a bare tempdir: no feature flags.
        assert!(meta.feature_flags.is_empty(), "{:?}", meta.feature_flags);
    }

    #[test]
    fn fast_metadata_with_cognitive_from_index_never_panics() {
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let meta = touring_hooks::shared::metadata_collector::FastMetadata::from_path(&file_path)
            .unwrap()
            .with_cognitive_from_index();

        // Returns self (no-op or populated) — no panic possible
        let _ = meta;
    }

    // ── Chain of enrichment ─────────────────────────────────────────────────────

    /// Full chain: with_language + with_feature_flags + with_cognitive_from_index.
    #[test]
    fn fast_metadata_full_enrichment_chain() {
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let meta = touring_hooks::shared::metadata_collector::FastMetadata::from_path(&file_path)
            .unwrap()
            .with_language()
            .with_feature_flags()
            .with_cognitive_from_index();

        // All enrichment methods applied without panic
        assert!(meta.language.is_some());
        // feature_flags may be empty but valid
        let _ = meta;
    }
}
