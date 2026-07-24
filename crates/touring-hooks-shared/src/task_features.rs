//! Task feature extraction for LinUCB routing.
//!
//! Converts task metadata (from TaskList `tool_result` JSON) into a 25-dimensional
//! feature vector compatible with [`touring_intelligence::rl::bandit::linucb::FEATURE_DIM`].
//!
//! # Feature Layout
//!
//! ```text
//! [0..3]   file_type stub (python=0, rust=1, ts=2, other=3) — one-hot, 4 dims
//! [4..6]   subject_size_bucket (short<80, medium<400, long>=400) — one-hot, 3 dims
//! [7..9]   cila_level_bucket (low=0..2, mid=3..4, high=5..6) — one-hot, 3 dims
//! [10..11] recent_error stub (placeholder zeros for future signal) — 2 dims
//! [12..18] cila_level one-hot (L0..L6) — 7 dims
//! [19]     error_count_session stub (0.0) — 1 dim
//! [20]     recent_tool_success stub (0.5 neutral) — 1 dim
//! [21..24] keyword-driven routing signals:
//!           [21] has_implement_keyword (1.0 = yes)
//!           [22] has_test_keyword
//!           [23] has_refactor_keyword
//!           [24] has_split_or_large_keyword
//! Total: 25 dimensions — matches FEATURE_DIM in touring-learning
//! ```
//!
//! All feature values are normalized to [0, 1]. Unknown or missing fields default to 0.
//! LinUCB is robust to zero-padded dimensions — theta learns zero weights when no signal.

use serde_json::Value;
use touring_intelligence::rl::bandit::linucb::FEATURE_DIM;

// ── Keyword sets for routing signal extraction ──────────────────────────────

const IMPLEMENT_KEYWORDS: &[&str] = &[
    "implement",
    "implementar",
    "criar",
    "create",
    "add ",
    "adicionar",
    "build ",
    "construir",
    "generate",
    "gerar",
    "scaffold",
];

const TEST_KEYWORDS: &[&str] = &[
    "test",
    "teste",
    "spec ",
    "e2e",
    "unit test",
    "integration test",
];

const REFACTOR_KEYWORDS: &[&str] = &[
    "refactor",
    "refatorar",
    "extract ",
    "migrat",
    "renam",
    "reorganiz",
];

const SPLIT_KEYWORDS: &[&str] = &[
    " e ", " and ", "multiple", "vários", "todos os", "all the", "entire", "complex",
];

// ── Sub-extractors (one per feature group) ──────────────────────────────────

/// Fills dims [4..6]: subject length bucket (one-hot).
///
/// short < 80, medium < 400, long >= 400.
#[allow(clippy::indexing_slicing)]
fn fill_size_bucket(features: &mut [f64; FEATURE_DIM], slen: usize) {
    if slen < 80 {
        features[4] = 1.0;
    } else if slen < 400 {
        features[5] = 1.0;
    } else {
        features[6] = 1.0;
    }
}

/// Fills dims [7..9]: cila_level bucket (one-hot) and dims [12..18]: cila_level L0..L6 (one-hot).
#[allow(clippy::indexing_slicing)]
fn fill_cila_features(features: &mut [f64; FEATURE_DIM], cila_level: usize) {
    // Bucket: low=0..2, mid=3..4, high=5+
    if cila_level <= 2 {
        features[7] = 1.0;
    } else if cila_level <= 4 {
        features[8] = 1.0;
    } else {
        features[9] = 1.0;
    }
    // One-hot L0..L6
    features[12 + cila_level.min(6)] = 1.0;
}

/// Returns 1.0 if any keyword in `kws` appears in `lc_subject`, else 0.0.
fn keyword_signal(lc_subject: &str, kws: &[&str]) -> f64 {
    if kws.iter().any(|kw| lc_subject.contains(kw)) {
        1.0
    } else {
        0.0
    }
}

/// Fills dims [21..24]: keyword-driven routing signals.
#[allow(clippy::indexing_slicing)]
fn fill_keyword_signals(features: &mut [f64; FEATURE_DIM], subject: &str) {
    if subject.is_empty() {
        return;
    }
    let lc = subject.to_lowercase();
    features[21] = keyword_signal(&lc, IMPLEMENT_KEYWORDS);
    features[22] = keyword_signal(&lc, TEST_KEYWORDS);
    features[23] = keyword_signal(&lc, REFACTOR_KEYWORDS);
    // dim 24: multi-part task signal — also triggers on long subjects (> 200 chars)
    let split_kw = keyword_signal(&lc, SPLIT_KEYWORDS);
    features[24] = if split_kw > 0.0 || subject.len() > 200 {
        1.0
    } else {
        0.0
    };
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Extracts a 25-dimensional feature vector from a task JSON object.
///
/// Accepts the shape from `TaskList tool_result.tasks[]`: fields `title`,
/// `description`, `status`, `cila_level`. Missing fields produce zero features.
/// All array indexing is guarded by `fill_*` sub-functions that use compile-time
/// constants within `[0, FEATURE_DIM)`.
pub fn extract_task_features(task: &Value) -> [f64; FEATURE_DIM] {
    debug_assert_eq!(
        FEATURE_DIM, 25,
        "FEATURE_DIM drift — update task_features.rs"
    );

    let mut features = [0.0_f64; FEATURE_DIM];

    // dim 3 = "other" type — task subjects don't map to a specific language
    #[allow(clippy::indexing_slicing)]
    {
        features[3] = 1.0;
    }

    let subject = task
        .get("title")
        .or_else(|| task.get("description"))
        .or_else(|| task.get("subject"))
        .or_else(|| task.get("task_subject"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    fill_size_bucket(&mut features, subject.len());

    let cila_level = task.get("cila_level").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    fill_cila_features(&mut features, cila_level);

    // dim 20: neutral tool success (no live signal yet)
    #[allow(clippy::indexing_slicing)]
    {
        features[20] = 0.5;
    }

    fill_keyword_signals(&mut features, subject);

    features
}

/// Convert a LinUCB arm index (0..NUM_ARMS=8) to a task-routing decision.
///
/// The existing LinUCB arms encode *context injection strategy*, which we
/// reinterpret as delegation signals:
///
/// | Arm | Original meaning | Task routing interpretation |
/// |-----|-----------------|----------------------------|
/// | 0: None | no context | ManualEdit — default |
/// | 1: Overview | AST overview | GeneratorStruct / GeneratorFn |
/// | 2: Gotcha | gotchas | ManualEdit (careful work) |
/// | 3: BlastRadius | impact analysis | SplitTask (high blast) |
/// | 4: Relations | import/caller | DelegateAgent |
/// | 5: OverviewGotcha | overview+gotcha | GeneratorFn (fn with gotchas) |
/// | 6: OverviewBlastRadius | overview+blast | GeneratorTrait (trait system) |
/// | 7: FullEnrichment | all context | DeferTask (needs full investigation) |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRoutingDecision {
    /// Standard manual edit — no hint emitted.
    ManualEdit,
    /// Delegate to touring-generator: struct kind.
    GeneratorStruct,
    /// Delegate to touring-generator: fn kind.
    GeneratorFn,
    /// Delegate to touring-generator: trait kind.
    GeneratorTrait,
    /// Delegate to a specialized agent via the Agent tool.
    DelegateAgent,
    /// Task should be split into smaller subtasks.
    SplitTask,
    /// Defer — EV low across all arms, discuss with operator first.
    DeferTask,
}

impl TaskRoutingDecision {
    /// Map a LinUCB arm index to a routing decision.
    pub fn from_arm_id(arm_id: usize) -> Self {
        match arm_id {
            0 => Self::ManualEdit,
            1 => Self::GeneratorStruct,
            2 => Self::ManualEdit, // Gotcha → careful manual work
            3 => Self::SplitTask,
            4 => Self::DelegateAgent,
            5 => Self::GeneratorFn,
            6 => Self::GeneratorTrait,
            _ => Self::DeferTask,
        }
    }

    /// Returns a non-empty routing hint string, or `None` for `ManualEdit`.
    ///
    /// The hint is prefixed with `[TOURING RL-ROUTER]` so Claude Code can
    /// visually distinguish it from static generator hints.
    pub fn hint(&self, subject: &str) -> Option<String> {
        let subject_short = &subject[..subject.len().min(80)];
        match self {
            Self::ManualEdit => None,
            Self::GeneratorStruct => Some(format!(
                "[TOURING RL-ROUTER] Generator candidate (struct/data): \
                run `touring generate plan-suggest --intent '{subject_short}'`"
            )),
            Self::GeneratorFn => Some(format!(
                "[TOURING RL-ROUTER] Generator candidate (fn/handler): \
                run `touring generate plan-suggest --intent '{subject_short}'`"
            )),
            Self::GeneratorTrait => Some(format!(
                "[TOURING RL-ROUTER] Generator candidate (trait/interface): \
                run `touring generate plan-suggest --intent '{subject_short}'`"
            )),
            Self::DelegateAgent => Some(format!(
                "[TOURING RL-ROUTER] Delegate to specialized agent: \
                subject='{subject_short}'"
            )),
            Self::SplitTask => Some(
                "[TOURING RL-ROUTER] Complex task — consider splitting: \
                run `touring decompose add <task_id> <subtask_id> \"<part>\"`"
                    .to_string(),
            ),
            Self::DeferTask => Some(
                "[TOURING RL-ROUTER] Low EV in all arms — defer or discuss with operator \
                before proceeding"
                    .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_features_implements_keyword() {
        let task = json!({"title": "implementar novo endpoint de autenticação"});
        let feats = extract_task_features(&task);
        // dim 21: implement keyword
        assert_eq!(feats[21], 1.0, "implement keyword → dim 21");
        // dim 22: no test keyword
        assert_eq!(feats[22], 0.0, "no test keyword → dim 22 zero");
        // dims 4..6: subject_size_bucket — short (< 80 chars)
        assert_eq!(feats[4], 1.0, "short subject → dim 4");
        // dim 3: always 'other' type
        assert_eq!(feats[3], 1.0, "file_type other → dim 3");
        // total dims
        assert_eq!(feats.len(), FEATURE_DIM);
    }

    #[test]
    fn test_extract_features_test_keyword() {
        let task = json!({"title": "add unit test for parser module"});
        let feats = extract_task_features(&task);
        assert_eq!(feats[22], 1.0, "test keyword → dim 22");
        assert_eq!(feats[21], 1.0, "add keyword → dim 21");
    }

    #[test]
    fn test_extract_features_refactor_keyword() {
        let task = json!({"description": "refactor decomposer.rs to use new DAG API"});
        let feats = extract_task_features(&task);
        assert_eq!(feats[23], 1.0, "refactor keyword → dim 23");
    }

    #[test]
    fn test_extract_features_split_candidate() {
        let task = json!({"title": "implementar todos os módulos do sistema de autenticação e autorização e sessão"});
        let feats = extract_task_features(&task);
        assert_eq!(feats[24], 1.0, "multi-part + len > 80 → dim 24");
    }

    #[test]
    fn test_extract_features_cila_level_one_hot() {
        let task = json!({"title": "fix typo", "cila_level": 3});
        let feats = extract_task_features(&task);
        // cila_level=3 → dim 12+3=15
        assert_eq!(feats[15], 1.0, "cila_level=3 → dim 15");
        // cila_level_bucket: mid (3..=4) → dim 8
        assert_eq!(feats[8], 1.0, "cila_level=3 → mid bucket → dim 8");
    }

    #[test]
    fn test_extract_features_empty_task() {
        let task = json!({});
        let feats = extract_task_features(&task);
        // dim 3 = 'other', dim 4 = short bucket (empty string), dim 7 = low cila, dim 12 = L0
        assert_eq!(feats[3], 1.0);
        assert_eq!(feats[4], 1.0);
        assert_eq!(feats[7], 1.0);
        assert_eq!(feats[12], 1.0);
        // All keyword dims zero
        assert_eq!(feats[21], 0.0);
        assert_eq!(feats[22], 0.0);
        assert_eq!(feats[23], 0.0);
        assert_eq!(feats[24], 0.0);
    }

    #[test]
    fn test_routing_decision_hint_manual_edit_is_none() {
        assert!(TaskRoutingDecision::ManualEdit.hint("fix typo").is_none());
    }

    #[test]
    fn test_routing_decision_hint_generator_struct() {
        let hint = TaskRoutingDecision::GeneratorStruct
            .hint("create new user model")
            .expect("GeneratorStruct always produces a hint");
        assert!(hint.contains("[TOURING RL-ROUTER]"), "hint has prefix");
        assert!(
            hint.contains("plan-suggest"),
            "hint references plan-suggest"
        );
    }

    #[test]
    fn test_routing_decision_from_arm_id_coverage() {
        // Ensure all 8 arms map to valid decisions
        for arm in 0..8_usize {
            let _ = TaskRoutingDecision::from_arm_id(arm);
        }
        // Out-of-range maps to DeferTask
        assert_eq!(
            TaskRoutingDecision::from_arm_id(99),
            TaskRoutingDecision::DeferTask
        );
    }

    #[test]
    fn test_features_neutral_tool_success() {
        let task = json!({"title": "any task"});
        let feats = extract_task_features(&task);
        // dim 20: neutral tool success = 0.5
        assert!((feats[20] - 0.5).abs() < 1e-9, "dim 20 = 0.5 neutral");
    }
}
