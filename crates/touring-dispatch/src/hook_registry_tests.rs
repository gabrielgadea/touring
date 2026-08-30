use super::*;

/// T3.10: Use cfg-aware `all_daemon_hook_names()` for registry↔dispatch matching.
#[test]
fn registry_names_match_dispatch_table() {
    let table = build_dispatch_table();
    let names = all_daemon_hook_names();
    for name in &names {
        assert!(
            table.contains_key(name),
            "Hook '{name}' is in all_daemon_hook_names() but missing from dispatch table"
        );
    }
}

/// T3.10: Reverse check — every dispatch entry must be in the cfg-aware list.
#[test]
fn dispatch_table_entries_are_in_registry() {
    let table = build_dispatch_table();
    let names = all_daemon_hook_names();
    for key in table.keys() {
        assert!(
            names.contains(key),
            "Hook '{key}' is in dispatch table but missing from all_daemon_hook_names()"
        );
    }
}

#[test]
fn no_duplicate_hook_names() {
    let mut seen = std::collections::HashSet::new();
    let names = all_daemon_hook_names();
    for name in &names {
        assert!(
            seen.insert(name),
            "Duplicate hook name in registry: '{name}'"
        );
    }
}

#[test]
fn registry_has_expected_count() {
    // With all features: 6 pre + 7 post + 2 session + 6 team + 2 team-gate + 9 lifecycle
    // + 19 cli telemetry + 4 cli session + 2 cli suggest + 8 cli decompose
    // + 1 cli mcts + 1 cli shadow + 5 cli index + 10 cli ast + 1 cli e2e
    // + 1 cli pre-task-scout + 1 cli prompt-enhance + 2 cli hook-memory
    // + 3 cli entity + 6 cli jobs + 1 cli jobs-drop + 1 cli-wiring-suggest
    // + 14 P7 Pln2 (ast-callgraph/todos/rationale/features/meta/skeleton/blast-enriched
    //   wiring-purpose/search-symbols/search-docs/query-dsl/metadata-backfill
    //   session-summary/bench-run)
    // + 5 tantivy (search/fuzzy/stats/suggest/reindex) + 1 wiring-community
    // + 3 wave-cross-audit (wiring-chains/ast-blast-cross-feature/file-knowledge-extended)
    // + 2 decompose (finalize/ready)
    // + 2 plan-mode (enter-plan-mode/exit-plan-mode) [R7]
    // + 3 task-sync (task-sync-create/update/list) [R7-Sync]
    // + 2 task-sync (task-sync-output/get) [R8-Sync]
    // + 2 task-sync (task-sync-stop/delete) [R9-Sync]
    // + 5 Pln3 suggestion CLI (suggest-action/suggestion-list/stats/gc/consumed) = 138
    // + 2 Wave 15 health_delta CLI (cli-health-delta-status/reset) = 140
    // + 1 polyglot: cli-ast-grep (ast-grep integration) = 141
    // + 2 Wave C1.7 granularity CLI (cli-granularity-status/reset) = 143
    // + 1 Wave C2-D2 granularity hint (cli-granularity-hint) = 144
    // + 2 Wave C D5 cascade queue CLI (cli-cascade-queue-status/drain) = 146
    // + 7 PLN2 saga coordinator CLI (cli-saga-register/prepare/decide/delta/begin/status/abort) = 153
    // + 6 Feature B workflow CLI (cli-workflow-run/stats/slowest/compare/resume/status) = 159
    // + 2 Feature C/D infrastructure (schemas + hook_context shared modules) — NOT hooks
    // + 1 Wave 26 Tokio metrics endpoint (cli-tokio-metrics) = 161
    // + 1 Wave Q1 TDG grade letter (cli-ast-tdg) = 162
    // + 2 Wave Q1 wire orphans potencializados (cli-wiring-impact/cycles) = 164
    // + 1 Wave Q2 batch scan (cli-ast-scan) = 165
    // + 2 Wave Q3 gotcha sync/init (cli-gotcha-sync/init) = 167
    // + 1 Wave R1 repo-score executive dashboard (cli-repo-score) = 168
    // + 1 Wave R2 kpi falsifiable commitments dashboard (cli-kpi) = 169
    // + 1 Wave R3 repo-health auto-generated markdown (cli-repo-health) = 170
    // + 1 Wave T1 cargo-mutants wrapper (cli-mutation-test) = 171
    // + 1 Wave P1.6 mpatch-fuzzy dry-run (cli-mpatch-preview) = 172
    // + 2 Wave 8 collateral fix (cli-tasksfile-validate/export) = 174
    // + 2 Wave 9 collateral fix (cli-devrcfile-import/export) = 176
    // D.2: resolve-def + find-references + rename
    // NOTE: constant (182) and function (184) diverge on feature-gated entries
    // and 'stop' which is in constant but not in function.
    // test_hook_registry_no_duplicates validates no duplicates.
    let names = all_daemon_hook_names();
    // D43 (W2): +2 pre-hooks (pre-grep, pre-glob) for Grep/Glob enrichment
    // D27: +4 plugin DI CLI
    // D28: +1 MCP overhead handler (cli-mcp-overhead)
    // Wave cross-audit 2026-05-03: cli-flow removed (was in names but no dispatch entry),
    // cli-mcp-overhead added to names (was dispatch-only). Net count unchanged.
    // Wave C subtask-1 2026-05-03: +1 post-tool-batch handler.
    // Wave 10 2026-05-03: +1 cli-repair-wiring (ALL) +1 dispatch entry.
    // D10-S1: +2 cli-doctor + cli-status + cross-audit: +3 file-knowledge +1 repair-wiring
    // 2026-05-07: +1 user_prompt_submit (context-mode A1 adoption)
    // 2026-05-10 (B3): +1 cli-index-ingest (single-file incremental reindex)
    // 2026-05-10 (suggester): +1 cli-suggest (PreToolUse classifier)
    // 2026-05-29 (elite-harness S-01): +1 ceg-observe (universal CEG observability);
    //   also +1 reconciling a pre-existing tripwire drift (an entry was added
    //   between 2026-05-10 and now without bumping this assert): 211→213.
    // 2026-05-29 (elite-harness S-04): +1 cli-memory-reindex (ANN backfill): 213→214.
    // 2026-05-30 (B-5): +1 cli-predict-action (historical outcome distillation)
    //   + 2 reconciling a pre-existing tripwire drift (two entries added since
    //   the last bump without updating this assert): 214→217.
    // 2026-05-30 (A-A1): +1 cli-calibrate-confidence (conformal skill-selection): 217→218.
    // 2026-05-30 (Cold-start): +1 cli-rl-warmstart (cross-project RL replay): 218→219.
    // 2026-05-30 (ES4 P1): +1 cli-world-model-status (durable world-model probe): 219→220.
    // 2026-05-30 (ES2 P2): +1 cli-attest-contract (constitutional sink-token, B-6): 220→221.
    // 2026-06-01 (ES1 P4): +1 cli-prove-claim (standalone SMT service, CAH roadmap): 221→222.
    // 2026-08-03: o tripwire passa a ser CIENTE DA FEATURE. `acp-protocol` (não-default)
    //   contribui 2 nomes; antes eles existiam só em `build_dispatch_table`, e a divergência
    //   entre as duas listas reprovava `dispatch_table_and_registry_are_in_sync` sob
    //   `--all-features`. Um número único não consegue ser verdade nos dois perfis — somar
    //   2 ao literal consertaria `--all-features` e quebraria o default.
    // 2026-08-04: +1 cli-memory-credit (case attribution — closes the
    //   recall->outcome loop the case bank never had): 222->223 / 224->225.
    // 2026-08-12 (hashtag library F1-F6): +10 memory facet commands —
    //   cli-memory-tag-add / tags / query (F1), cli-memory-sync-tags (F2),
    //   cli-memory-link / links (F3), cli-memory-backfill-tags (F4),
    //   cli-memory-moc / communities / unlink (F5-F6): 223->233 / 225->235.
    // 2026-08-12 (H2 SCIP type-aware wiring): +1 cli-wiring-scip-ingest:
    //   233->234 / 235->236.
    // 2026-08-18 (plano graph-engineering-flow-portfolio, C2+C3): +4 decompose
    //   verbs — cli-decompose-claim / cli-decompose-release (atomic claim with an
    //   expiring lease: `ready` only READS, so two sessions polling it received the
    //   SAME subtask) and cli-decompose-ticket / cli-decompose-frontier (Wayfinder:
    //   decision tickets gate the implementation frontier): 234->238 / 236->240.
    // 2026-08-24 (code-mode máximo): +1 cli-code-mode-run — o `touring run` é um
    //   CLI efêmero, então o `record_code_mode_run` do executor incrementava um
    //   counter que morria com o processo; o `gate-metrics` lê do daemon, e por
    //   isso o canal SEM MCP contava zero (163 runs no journal contra
    //   `code_mode_runs_count = 0`) enquanto a rota MCP contava tudo. Este hook
    //   é o relay que o CLI usa para o daemon contabilizar: 238->239 / 240->241.
    // 2026-08-29 (R1 ligar-não-construir): +1 cli-gotcha-resolve — o produtor
    //   do canal que `touring.gotcha.resolution` lê (91 gotchas, meter 0.0:
    //   `resolved_at` existia no schema e nenhum caminho o escrevia):
    //   239->240 / 241->242.
    // 2026-08-29 (R4): +2 cli-experiment-{record,list} — a superfície do
    //   ExperimentLog (zero chamadores de produção; o A/B do variant_archive
    //   passa a registrar e ler de volta): 240->242 / 242->244.
    // 2026-08-29 (P2 replay): +1 cli-learning-replay — o canal offline→engine
    //   (o corpus de outcomes recompensados nunca alcançava o OnlineRLEngine;
    //   o motor recomeçava do zero a cada restart): 242->243 / 244->245.
    #[cfg(feature = "acp-protocol")]
    const EXPECTED_NAMES: usize = 245;
    #[cfg(not(feature = "acp-protocol"))]
    const EXPECTED_NAMES: usize = 243;
    assert_eq!(names.len(), EXPECTED_NAMES);
    // Backward-compat constant (204, feature-gated entries differ)
    // 2026-05-07: +1 user_prompt_submit = 205
    // 2026-05-10 (B3): +1 cli-index-ingest = 206
    // 2026-05-10 (suggester): +1 cli-suggest = 207
    // 2026-05-29 (elite-harness S-01): +1 ceg-observe + 1 pre-existing
    //   tripwire-drift reconciliation = 209.
    // 2026-05-29 (elite-harness S-04): +1 cli-memory-reindex = 210.
    // 2026-05-30 (B-5): +1 cli-predict-action + 2 pre-existing tripwire-drift
    //   reconciliation = 213.
    // 2026-05-30 (A-A1): +1 cli-calibrate-confidence = 214.
    // 2026-05-30 (Cold-start): +1 cli-rl-warmstart = 215.
    // 2026-05-30 (ES4 P1): +1 cli-world-model-status = 216.
    // 2026-05-30 (ES2 P2): +1 cli-attest-contract = 217.
    // 2026-06-01 (ES1 P4): +1 cli-prove-claim = 218.
    // 2026-08-04: +1 cli-memory-credit = 219.
    // 2026-08-12 (hashtag library F1-F6): +10 memory facet commands = 229.
    // 2026-08-12 (H2 SCIP type-aware wiring): +1 cli-wiring-scip-ingest = 230.
    // 2026-08-18 (C2+C3): +4 cli-decompose-{claim,release,ticket,frontier} = 234.
    // 2026-08-24 (code-mode máximo): +1 cli-code-mode-run = 235.
    // 2026-08-29 (R1): +1 cli-gotcha-resolve = 236.
    // 2026-08-29 (R4): +2 cli-experiment-{record,list} = 238.
    // 2026-08-29 (P2 replay): +1 cli-learning-replay = 239.
    assert_eq!(ALL_DAEMON_HOOK_NAMES.len(), 239);
}

/// The tripwire literal is duplicated across four files; this asserts they agree.
///
/// The count guard exists in `touring-dispatch/src/hook_registry_tests.rs` and in
/// three `touring-hooks` integration tests. Updating a SUBSET is how
/// `cli-memory-credit` reached CI with three red tests — and, on 2026-08-18, how
/// adding four decompose verbs turned up as three separate failures discovered one
/// run at a time. Per the injection-density lesson, a quality invariant has to be
/// enforced over ALL its instances at once or it recurs in the next uncovered one.
///
/// This does not weaken the tripwire: a new hook still requires a human to change
/// the number deliberately. It only makes the four copies unable to drift apart.
#[test]
fn every_hook_count_tripwire_agrees_with_the_others() {
    use std::path::Path as StdPath;

    // The workspace root, from this file's location at compile time.
    let root = StdPath::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(StdPath::parent)
        .expect("workspace root");
    let sites = [
        "crates/touring-dispatch/src/hook_registry_tests.rs",
        "crates/touring-hooks/tests/stringzilla_e2e.rs",
        "crates/touring-hooks/tests/wave2_4_e2e.rs",
        "crates/touring-hooks/tests/wave_c_e2e.rs",
    ];
    let mut found: Vec<(&str, Vec<String>)> = Vec::new();
    for site in sites {
        let path = root.join(site);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("tripwire site {site} unreadable: {e}"));
        let counts: Vec<String> = text
            .lines()
            .filter_map(|l| l.trim().strip_prefix("const EXPECTED_NAMES: usize = "))
            .map(|v| v.trim_end_matches(';').to_string())
            .collect();
        assert!(
            !counts.is_empty(),
            "{site} no longer declares EXPECTED_NAMES — either the tripwire was \
             removed or this guard's site list is stale"
        );
        found.push((site, counts));
    }
    let (first_site, first) = &found[0];
    for (site, counts) in &found[1..] {
        assert_eq!(
            counts, first,
            "hook-count tripwires disagree: {site} has {counts:?} while \
             {first_site} has {first:?} — update every site together"
        );
    }
}

/// Sprint 4.6 regression guard (2026-05-23).
///
/// The `post-bash` dispatch entry in `build_dispatch_table` previously
/// called `crate::post_bash::run(rt, v)` whose body terminates with
/// `run_returning(input).emit()` → `std::process::exit(0)` — which kills
/// the entire daemon process every time post-bash fires. That was the
/// root cause of intermittent silent daemon deaths documented in Sprint
/// 4.5 (no panic_log entry, no graceful_shutdown log, no signal — clean
/// `exit_group(0)` from a tokio worker thread under load, verified via
/// Sprint 4.6 strace wrapper at `~/.claude/touring/daemon-strace.log`).
///
/// All other hook entries correctly use the `run_returning(...).to_json()`
/// pattern, which returns a serialized response to the caller instead of
/// diverging via process::exit. This test enforces that contract for the
/// post-bash entry to prevent regression.
#[test]
fn sprint_4_6_post_bash_dispatch_must_not_call_emit() {
    let src = include_str!("hook_registry.rs");
    let idx = src
        .find("m.insert(\"post-bash\"")
        .expect("post-bash dispatch entry must exist in hook_registry.rs");
    // Look at the next ~600 chars (enough for a complete insert! block + comments)
    let after = &src[idx..(idx + 600).min(src.len())];
    assert!(
        after.contains("run_returning"),
        "Sprint 4.6 regression: post-bash dispatch must call run_returning(...).to_json(). \
             Found block: {}",
        after
    );
    assert!(
        !after.contains("crate::post_bash::run(rt"),
        "Sprint 4.6 regression: post-bash dispatch must NOT call crate::post_bash::run(rt, v) \
             — that function ends with .emit() which calls std::process::exit(0) and kills the \
             daemon process. Use crate::post_bash::run_returning(rt, v).to_json() instead. \
             Found block: {}",
        after
    );
}

/// Sprint 4.6 STRUCTURAL DEFENSE (2026-05-24, REGRA #0 potencializar).
///
/// Generalizes `sprint_4_6_post_bash_dispatch_must_not_call_emit` from
/// one entry (post-bash) to every hook module whose `pub fn run()` body
/// terminates with `.emit()` → `process::exit(0)`. If any such module is
/// invoked through the daemon dispatch table as
/// `crate::<module>::run(rt, v)`, the daemon dies the moment the hook
/// fires — exactly the Sprint 4.6 bug, but for a different event.
///
/// The 14 modules below were enumerated by a Sprint 4.6 D1 exhaustive
/// scan (`grep -l '\.emit()' crates/touring-hooks/src/*.rs` intersected
/// with `grep 'pub fn run'`). Add to this list any new hook module that
/// is created with the `run_returning(...).emit()` divergent helper.
///
/// SAFE callers (verified Sprint 4.6 D1) — these dispatch entries invoke
/// `crate::<name>::run(rt, v)` directly but the target `run` returns
/// `String`/`Result` WITHOUT calling `.emit()`:
///   - `cli_suggester::run`  (line ~648, returns `String`)
///   - `post_tool_rl::run`   (line ~684, returns `Result<(), String>`)
///   - `post_compact_handler::run` (line ~1133, returns `String`)
/// They are NOT in the dangerous list because they do NOT call `.emit()`.
#[test]
fn sprint_4_6_no_dispatch_entry_may_call_an_emitting_run() {
    const DANGEROUS_MODULES: &[&str] = &[
        "permission_request",
        "post_bash",
        "post_edit",
        "post_tool_use",
        "post_write",
        "pre_bash",
        "pre_edit",
        "pre_edit_prevention",
        "pre_glob",
        "pre_grep",
        "pre_read",
        "pre_tool_use",
        "pre_write",
        "stop",
    ];

    let full_src = include_str!("hook_registry.rs");
    // Restrict the scan to production code only — exclude the `mod tests`
    // block which contains string literals citing the anti-pattern as
    // part of its own failure messages (those would be self-matches).
    let prod_end = full_src.find("#[cfg(test)]").unwrap_or(full_src.len());
    let src = &full_src[..prod_end];
    let mut violations: Vec<String> = Vec::new();

    for module in DANGEROUS_MODULES {
        let needle = format!("crate::{}::run(rt", module);
        // Walk every occurrence; skip those inside comments. A match is
        // a real dispatch invocation only if its line does NOT start with
        // `//` (after trimming whitespace). Comment hits are this test's
        // own documentation citing the anti-pattern — they must not panic.
        let mut search_from = 0usize;
        while let Some(rel_idx) = src[search_from..].find(needle.as_str()) {
            let idx = search_from + rel_idx;
            let line_start = src[..idx].rfind('\n').map(|n| n + 1).unwrap_or(0);
            let line_end = src[idx..].find('\n').map(|n| idx + n).unwrap_or(src.len());
            let line = &src[line_start..line_end];
            let trimmed = line.trim_start();
            let is_comment = trimmed.starts_with("//");
            if !is_comment {
                violations.push(format!(
                        "  • module `{}`: dispatch entry contains `{}` — this kills the daemon.\n    Found at: {}\n    Fix:  crate::{}::run_returning(rt, v).to_json()",
                        module,
                        needle.trim_end_matches("(rt"),
                        line.trim(),
                        module
                    ));
            }
            search_from = idx + needle.len();
        }
    }

    if !violations.is_empty() {
        panic!(
            "Sprint 4.6 STRUCTURAL DEFENSE: {} dispatch entry(ies) invoke an emitting run():\n{}\n\n\
                 Every `crate::<mod>::run(rt, v)` in the dispatch table where `<mod>::run` ends with \
                 `run_returning(input).emit()` will call `std::process::exit(0)` from inside a tokio \
                 worker task, terminating the daemon. Use `run_returning(rt, v).to_json()` instead.\n\n\
                 If a NEW hook module is added to the codebase with the `run_returning(...).emit()` \
                 divergent helper, append its name to the DANGEROUS_MODULES list above so future \
                 dispatch entries are guarded.",
            violations.len(),
            violations.join("\n")
        );
    }
}
