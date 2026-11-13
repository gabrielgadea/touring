//! The eight canonical SDK hook names live in two crates that cannot see each other:
//! `touring_code::sdk::HookName::ALL` (the source of truth, what the KPI credits) and
//! `touring_quality::builtins::best_practices::SDK_HOOK_NAMES` (the copy `BestPracticesGate`
//! counts `signal_use` against — `touring-quality` must stay standalone-buildable, so it may
//! not depend on `touring-code`). `touring-server` sees both, so the guard is here: a hook added,
//! renamed or removed in one list and not in the other fails this test instead of letting the
//! gate and the KPI disagree about what "used" means (cross-audit 12/11/2026, F5).

use std::collections::BTreeSet;

use touring_code::sdk::HookName;
use touring_quality::builtins::best_practices::SDK_HOOK_NAMES;

#[test]
fn sdk_hook_names_match_the_canonical_set() {
    let canonical: BTreeSet<&str> = HookName::ALL.iter().map(|h| h.as_str()).collect();
    let copy: BTreeSet<&str> = SDK_HOOK_NAMES.iter().copied().collect();
    assert_eq!(
        SDK_HOOK_NAMES.len(),
        copy.len(),
        "SDK_HOOK_NAMES lists a name twice: {SDK_HOOK_NAMES:?}"
    );
    assert_eq!(
        copy,
        canonical,
        "BestPracticesGate counts signal_use against a list that drifted from \
         touring_code::sdk::HookName::ALL: only in the gate: {:?}; only in the SDK: {:?}",
        copy.difference(&canonical).collect::<Vec<_>>(),
        canonical.difference(&copy).collect::<Vec<_>>(),
    );
}
