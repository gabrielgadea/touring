//! CILA (Cognitive Intent-Level Architecture) budget helpers.
//!
//! Centralises the three duplicated `cila_budget_*` functions from
//! `pre_read`, `pre_edit`, and `pre_write` into a single generic helper.

/// Resolve a CILA token budget given an env-var key prefix and default values.
///
/// Checks `<env_prefix>_L0`, `<env_prefix>_L2`, `<env_prefix>_L4` for overrides.
/// Falls back to the provided defaults when no env var is set.
#[inline]
pub fn cila_budget(cila_level: u8, env_prefix: &str, low: usize, mid: usize, high: usize) -> usize {
    let env_key = match cila_level {
        0 | 1 => format!("{env_prefix}_L0"),
        2 | 3 => format!("{env_prefix}_L2"),
        _ => format!("{env_prefix}_L4"),
    };
    if let Ok(val) = std::env::var(&env_key)
        && let Ok(n) = val.parse::<usize>()
    {
        return n;
    }
    match cila_level {
        0 | 1 => low,
        2 | 3 => mid,
        _ => high,
    }
}

/// Budget for `pre_read` hooks (conservative — read context is smaller).
///
/// L0-L1: 800 | L2-L3: 2000 | L4+: 4000
#[inline]
pub fn cila_budget_read(cila_level: u8) -> usize {
    cila_budget(cila_level, "TOURING_CILA_BUDGET", 800, 2000, 4000)
}

/// Budget for `pre_edit` hooks (50% larger than read — edit context needs more).
///
/// L0-L1: 1200 | L2-L3: 3000 | L4+: 6000
#[inline]
pub fn cila_budget_edit(cila_level: u8) -> usize {
    cila_budget(cila_level, "TOURING_CILA_BUDGET_EDIT", 1200, 3000, 6000)
}

/// Budget for `pre_write` hooks (same as edit).
///
/// L0-L1: 1200 | L2-L3: 3000 | L4+: 6000
#[inline]
pub fn cila_budget_write(cila_level: u8) -> usize {
    cila_budget(cila_level, "TOURING_CILA_BUDGET_WRITE", 1200, 3000, 6000)
}

// ── L7-B Alpha: Enrichment Gate ──────────────────────────────────────────

/// L7-B Alpha: CILA-gated enrichment policy.
///
/// Decides whether the expensive context enrichment (knowledge DB queries,
/// NLP entity extraction, gotcha scan, signal pipeline) should run for a
/// given tool invocation.
///
/// # Rationale
///
/// Enrichment is expensive (10-100ms per hook). Running it globally degrades
/// latency of reflexive L0/L1 hooks where context isn't consumed anyway.
/// Running it never wastes the cognitive payoff at L2+ where Claude can
/// actually leverage the enriched signals.
///
/// # Gating Layers
///
/// 1. **Pipeline active**: `enrichment_active == true` (set at daemon startup
///    via `HookRuntime::trigger_enrichment`)
/// 2. **CILA level**: `cila_level >= 2` (L0 = Reflexo, L1 = Associação don't
///    benefit; L2+ Cognição/Orquestração/Self-mod do)
/// 3. **Tool filter**: mutation tools only (Read/Grep/Glob are fast-path
///    reads that don't need heavy enrichment)
///
/// # Why not use `touring_cortex::enrichment::EnrichmentPolicy`?
///
/// `touring-cortex` depends on `touring-hooks` (inverted). Importing from
/// cortex here would create a circular dependency. The logic is duplicated
/// intentionally — 10 lines is cheaper than a new crate or refactor.
///
/// # Example
///
/// ```ignore
/// use touring_hooks::shared::cila::should_enrich;
///
/// let enrich_ok = should_enrich(
///     runtime.enrichment_active,
///     cila_level,
///     "Edit",
/// );
/// if enrich_ok {
///     // Run expensive compose_edit_context
/// } else {
///     // Fast-path: skip DB query, return minimal context
/// }
/// ```
#[inline]
pub fn should_enrich(enrichment_active: bool, cila_level: u8, tool_name: &str) -> bool {
    // Layer 1: Pipeline must be active (daemon fully initialized)
    if !enrichment_active {
        return false;
    }
    // Layer 2: CILA level gate (L0/L1 fast-path, L2+ full)
    if cila_level < 2 {
        return false;
    }
    // Layer 3: Tool filter — only mutation tools benefit from enrichment
    matches!(
        tool_name,
        "Edit"
            | "Write"
            | "NotebookEdit"
            | "MultiEdit"
            | "pre-edit"
            | "pre-write"
            | "pre-bash"
            | "TaskCreate"
            | "TaskUpdate"
    )
}

/// L7-B Alpha: Check if enrichment is mandatory at L4+ levels.
///
/// At L4+ (self-modifying or multi-agent workflows), enrichment runs for
/// ALL tools regardless of the tool filter — context coherence is critical
/// for high-order cognitive coordination.
#[inline]
pub fn is_enrichment_mandatory(enrichment_active: bool, cila_level: u8) -> bool {
    enrichment_active && cila_level >= 4
}

#[cfg(test)]
mod tests {
    use super::*;

    // Serializes the env-var-mutating tests below. `set_var`/`remove_var` mutate the
    // process-global environment (UB under concurrent access — the reason edition 2024
    // marks them `unsafe`), and several of these tests share the `TOURING_TEST_CILA_L0`
    // key, so they must not run on parallel test threads (the default).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_read_defaults() {
        assert_eq!(cila_budget_read(0), 800);
        assert_eq!(cila_budget_read(1), 800);
        assert_eq!(cila_budget_read(2), 2000);
        assert_eq!(cila_budget_read(3), 2000);
        assert_eq!(cila_budget_read(4), 4000);
        assert_eq!(cila_budget_read(5), 4000);
    }

    #[test]
    fn test_edit_defaults() {
        assert_eq!(cila_budget_edit(0), 1200);
        assert_eq!(cila_budget_edit(2), 3000);
        assert_eq!(cila_budget_edit(4), 6000);
    }

    #[test]
    fn test_write_defaults() {
        assert_eq!(cila_budget_write(0), 1200);
        assert_eq!(cila_budget_write(3), 3000);
        assert_eq!(cila_budget_write(6), 6000);
    }

    #[test]
    fn test_env_override_l0() {
        let _env = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Valid numeric env-var override must be respected for L0/L1.
        // Remove first to ensure test isolation.
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_TEST_CILA_L0") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_TEST_CILA_L0", "9999") };
        let budget_l0 = cila_budget(0, "TOURING_TEST_CILA", 800, 2000, 4000);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_TEST_CILA_L0") };
        assert_eq!(budget_l0, 9999);
    }

    #[test]
    fn test_env_override_l2() {
        let _env = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Valid numeric env-var override must be respected for L2/L3.
        // Remove first to ensure test isolation.
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_TEST_CILA_L2") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_TEST_CILA_L2", "5555") };
        let budget_l2 = cila_budget(2, "TOURING_TEST_CILA", 800, 2000, 4000);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_TEST_CILA_L2") };
        assert_eq!(budget_l2, 5555);
    }

    #[test]
    fn test_env_override_l4() {
        let _env = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Valid numeric env-var override must be respected for L4+.
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_TEST_CILA_L4", "7777") };
        let budget_l4 = cila_budget(4, "TOURING_TEST_CILA", 800, 2000, 4000);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_TEST_CILA_L4") };
        assert_eq!(budget_l4, 7777);
    }

    #[test]
    fn test_env_override_invalid_parse_falls_back_to_default() {
        let _env = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Non-numeric env value must fall back to the hardcoded default.
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_TEST_CILA_L0", "not_a_number") };
        let budget_fallback = cila_budget(0, "TOURING_TEST_CILA", 800, 2000, 4000);
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_TEST_CILA_L0") };
        assert_eq!(
            budget_fallback, 800,
            "invalid env value should fall back to default 800"
        );
    }

    #[test]
    fn test_level_1_maps_to_l0_bucket() {
        // L1 shares the same bucket as L0 (both yield the 'low' default).
        assert_eq!(
            cila_budget(1, "TOURING_TEST_CILA_NOMATCH", 100, 200, 300),
            100
        );
    }

    #[test]
    fn test_level_3_maps_to_l2_bucket() {
        // L3 shares the same bucket as L2 (both yield the 'mid' default).
        assert_eq!(
            cila_budget(3, "TOURING_TEST_CILA_NOMATCH", 100, 200, 300),
            200
        );
    }

    #[test]
    fn test_level_6_maps_to_l4_bucket() {
        // L6 (multi-agent) falls in the L4+ bucket.
        assert_eq!(
            cila_budget(6, "TOURING_TEST_CILA_NOMATCH", 100, 200, 300),
            300
        );
    }
}
