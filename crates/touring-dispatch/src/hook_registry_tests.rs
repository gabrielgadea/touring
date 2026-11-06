use super::*;

// ── One source of truth (01/11/2026) ──────────────────────────────────────────────
//
// The names and the membership the thin client uses are DERIVED from the dispatch table.
// These tests therefore go through the interface the client calls (`is_daemon_hook`) and
// guard what a derived list can still get wrong: a name declared twice, a wrong membership
// verdict, the scan itself going blind.

/// The production source of `hook_registry.rs`: everything before the trailing
/// `#[cfg(test)] #[path = …] mod tests;`.
///
/// NOT everything before the FIRST `#[cfg(test)]`: the `task-completed` handler inside
/// `build_dispatch_table` carries a `#[cfg(test)]` statement, so cutting there reads under
/// half the file and silently skips every entry declared after it.
fn registry_production_source() -> &'static str {
    let full = include_str!("hook_registry.rs");
    &full[..full.rfind("#[cfg(test)]").unwrap_or(full.len())]
}

/// The literal name of every `m.insert("<name>", …)` in `build_dispatch_table`, in source
/// order — every declaration, whatever `#[cfg(feature)]` gates it.
fn declared_hook_names() -> Vec<&'static str> {
    let src = registry_production_source();
    let start = src
        .find("pub fn build_dispatch_table")
        .expect("build_dispatch_table must exist in hook_registry.rs");
    let mut rest = &src[start..];
    let mut names = Vec::new();
    while let Some(at) = rest.find("m.insert(") {
        rest = &rest[at + "m.insert(".len()..];
        if let Some(literal) = rest.trim_start().strip_prefix('"')
            && let Some(end) = literal.find('"')
        {
            names.push(&literal[..end]);
        }
    }
    names
}

/// Every name the daemon answers routes to a handler, and the predicate the thin client
/// calls (`is_daemon_hook`, `main.rs` fast-path) agrees — the membership test and the
/// dispatch are one fact, not two lists.
#[test]
fn every_daemon_hook_name_routes_to_a_handler_through_the_predicate() {
    let names = all_daemon_hook_names();
    assert!(!names.is_empty(), "an empty registry would pass vacuously");
    for name in names {
        assert!(
            is_daemon_hook(name),
            "'{name}' is a daemon hook name but is_daemon_hook rejects it"
        );
        assert!(
            dispatch_table().get(name).is_some(),
            "'{name}' is a daemon hook name but has no handler in the dispatch table"
        );
    }
}

/// The names ARE the table's keys — sorted (so the order is deterministic) and unique —
/// and the shared per-process cell holds exactly what `build_dispatch_table` declares.
#[test]
fn names_are_exactly_the_dispatch_table_keys() {
    let names = all_daemon_hook_names();
    assert!(
        names.windows(2).all(|w| w[0] < w[1]),
        "all_daemon_hook_names must be strictly sorted (sorted + unique)"
    );
    let fresh: std::collections::BTreeSet<&str> = build_dispatch_table().keys().copied().collect();
    let listed: std::collections::BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(
        listed, fresh,
        "all_daemon_hook_names must list exactly the keys build_dispatch_table declares"
    );
    assert_eq!(dispatch_table().len(), names.len());
}

/// A name inserted twice is silent in a `HashMap` — the later handler overwrites the earlier
/// and the table just has one key — so the names derived from it cannot show it. This reads
/// the declarations instead, and checks the scan sees every key the table has (a scan that
/// found nothing would pass the duplicate check vacuously).
#[test]
fn hook_names_are_declared_once() {
    let declared = declared_hook_names();
    let mut seen = std::collections::HashSet::new();
    for name in &declared {
        assert!(
            seen.insert(*name),
            "hook '{name}' is inserted into the dispatch table twice — the later handler \
             would silently replace the earlier one"
        );
    }
    for name in all_daemon_hook_names() {
        assert!(
            seen.contains(name),
            "the declaration scan missed '{name}' — update `declared_hook_names`"
        );
    }
}

/// Six hooks the dispatch table served that the old hand-written list never named (plus the
/// two `acp-protocol` ones under that feature), so `touring-hook` never routed them to the
/// daemon and no test saw it. They are members now, by construction.
#[test]
fn hooks_the_hand_written_list_dropped_are_members() {
    for name in [
        "cli-ast-callgraph",
        "cli-decompose-archive",
        "cli-decompose-reconcile-stages",
        "cli-mcp-overhead",
        "cli-ssr-apply",
        "cli-ssr-status",
    ] {
        assert!(
            is_daemon_hook(name),
            "'{name}' must take the daemon fast-path"
        );
    }
    for name in ["cli-acp-discover", "cli-acp-message"] {
        // Feature-aware, as the table is: the verdict follows the `acp-protocol` gate.
        assert_eq!(
            is_daemon_hook(name),
            cfg!(feature = "acp-protocol"),
            "'{name}' membership must follow the acp-protocol feature"
        );
    }
}

/// Subcommands the standalone path answers on its own are not daemon hooks, and neither is a
/// name nobody registered — membership is not "anything non-empty".
#[test]
fn stateless_subcommands_are_not_daemon_hooks() {
    for name in [
        "prompt-enhance",
        "qa-syntax",
        "classify",
        "pii-scan",
        "no-such-hook",
        "",
    ] {
        assert!(!is_daemon_hook(name), "'{name}' must not be a daemon hook");
    }
}

/// The table holds hook events AND the CLI's query handlers (`cli-…`); `is_rpc_endpoint` is
/// the line between them. The namespace is the rule, `cli-suggest` its only hook, and a stale
/// exception (an entry that is no table key, or not in the namespace) must fail here rather
/// than silently turn a hook into an "endpoint" — or the reverse.
#[test]
fn rpc_endpoints_are_the_cli_namespace_minus_its_hooks() {
    let names = all_daemon_hook_names();
    let rpc: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| is_rpc_endpoint(n))
        .collect();
    assert!(
        !rpc.is_empty() && rpc.contains(&"cli-doctor"),
        "the CLI query handlers are RPC endpoints (an empty set would pass vacuously)"
    );
    for name in HOOKS_IN_RPC_NAMESPACE {
        assert!(
            name.starts_with(RPC_NAMESPACE_PREFIX),
            "'{name}' is listed as an exception to the prefix rule but is outside the prefix"
        );
        assert!(
            is_daemon_hook(name),
            "'{name}' is listed as a hook in the cli- namespace but is no table key"
        );
        assert!(
            !is_rpc_endpoint(name),
            "'{name}' is a hook, not an endpoint"
        );
    }
    for name in names
        .iter()
        .filter(|n| !n.starts_with(RPC_NAMESPACE_PREFIX))
    {
        assert!(
            !is_rpc_endpoint(name),
            "'{name}' is outside the cli- namespace and cannot be an RPC endpoint"
        );
    }
    for name in [
        "pre-bash",
        "session-start",
        "user_prompt_submit",
        "no-such-hook",
        "",
    ] {
        assert!(!is_rpc_endpoint(name), "'{name}' is no RPC endpoint");
    }
}

/// Answering membership builds the table in the short-lived `touring-hook` process, so the
/// cost is measured, not assumed: the median of 101 builds, in the (pessimistic) debug test
/// profile, stays under 1 ms — the line above which a compile-time name list would be
/// needed instead. Prints the figures (`--nocapture`): the first membership query in this
/// process (cold only when this test runs alone) and the median build, in microseconds.
#[test]
fn building_the_table_to_answer_membership_is_cheap() {
    let first = std::time::Instant::now();
    std::hint::black_box(is_daemon_hook("pre-bash"));
    let first_us = first.elapsed().as_secs_f64() * 1e6;

    let mut samples: Vec<f64> = (0..101)
        .map(|_| {
            let t = std::time::Instant::now();
            let table = build_dispatch_table();
            let us = t.elapsed().as_secs_f64() * 1e6;
            std::hint::black_box(&table);
            us
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    let median_us = samples[samples.len() / 2];
    eprintln!(
        "[hook_registry] first membership query {first_us:.1} us; build_dispatch_table \
         median {median_us:.1} us (min {:.1}, max {:.1}, n=101, debug profile)",
        samples[0],
        samples[samples.len() - 1]
    );
    assert!(
        median_us < 1000.0,
        "building the dispatch table costs {median_us:.1} us (median) — over the 1 ms budget \
         for the thin client; derive a compile-time name list instead"
    );
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
    // NOTE (01/11/2026): the hand-written ALL_DAEMON_HOOK_NAMES constant is gone — the names
    // are the dispatch table's keys, so the count below is the table's size.
    // `hook_names_are_declared_once` validates no duplicates.
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
    // 2026-09-13 (I13, análise Graft §27 opção B): +1 cli-index-why — o rebuild já
    //   contava e amostrava os arquivos recusados (`oversized_skipped`, ≤10 paths),
    //   mas a resposta era transiente; `touring index why <path>` aplica as regras
    //   do próprio walker a um arquivo, sob demanda, e nomeia a exclusão
    //   (skipped_dir / unsupported_extension / oversized / …): 243->244 / 245->246.
    // 2026-09-20: +1 cli-decompose-reconcile-stages — fecha estagio cujos
    //   irmaos todos terminaram (os 74 ::scout que o closer nunca vera):
    //   246->247 / 248->249 (o e7f47c1d adicionou o handler e este comentário
    //   SEM bump das constantes — 5 dias vermelho; corrigido 25/09, REGRA #21).
    // 2026-09-19: +1 cli-decompose-archive — `archive_completed_tasks` existia,
    //   estava testada e tinha ZERO chamadores de produção, então nenhuma task
    //   terminal jamais recebia `archived_at` (NULL em 381 de 381, medido).
    //   O handler é a rota que faltava (REGRA #0): 244->245 / 246->247.
    #[cfg(feature = "acp-protocol")]
    const EXPECTED_NAMES: usize = 249;
    #[cfg(not(feature = "acp-protocol"))]
    const EXPECTED_NAMES: usize = 247;
    assert_eq!(names.len(), EXPECTED_NAMES);
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

    // Restrict the scan to production code only — exclude the `mod tests`
    // block which contains string literals citing the anti-pattern as
    // part of its own failure messages (those would be self-matches).
    // `registry_production_source` cuts at the LAST `#[cfg(test)]`: this scan used to cut at
    // the FIRST, which sits inside the `task-completed` handler (a `#[cfg(test)]` statement) —
    // so it read under half of the file and never saw the entries declared after it.
    let src = registry_production_source();
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

// ── C4 (2026-10-17): guards vs advisories ─────────────────────────────────────────

/// Handler sources whose deny is a helper of another hook, not a hook of its own.
const DENY_HELPER_FILES: &[&str] = &["vgp-execucao"];

/// Production part of a source file: everything before its first `#[cfg(test)]`.
fn production_part(src: &str) -> &str {
    src.split("#[cfg(test)]").next().unwrap_or(src)
}

/// The set of hooks whose handler source can deny is DERIVED from the code, so the
/// declared [`GUARD_HOOK_NAMES`] cannot drift: a new denying handler that is not declared a
/// guard would stay fail-open on a degraded stdin — exactly the defect C4 closes.
#[test]
fn guard_set_matches_the_handlers_that_can_deny() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../touring-hook-handlers/src/hooks");
    let mut derived: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read handler source");
        let prod = production_part(&src);
        let can_deny = prod.contains("HookResponse::Deny")
            || prod.contains("build_deny(")
            || prod.contains("build_deny_with_context(")
            || prod.contains("validation_deny(");
        if !can_deny {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("utf8 stem")
            .replace('_', "-");
        if DENY_HELPER_FILES.contains(&name.as_str()) {
            continue;
        }
        derived.insert(name);
    }
    let declared: std::collections::BTreeSet<String> =
        GUARD_HOOK_NAMES.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        derived, declared,
        "GUARD_HOOK_NAMES must be exactly the hooks whose handler source can deny"
    );
}

/// Every guard is a registered hook (a typo would make the guard branch dead code), and the
/// advisories — the rest of the registry — are not guards.
#[test]
fn guards_are_registered_hooks_and_advisories_are_not_guards() {
    for g in GUARD_HOOK_NAMES {
        assert!(
            is_daemon_hook(g),
            "guard '{g}' is not a registered daemon hook"
        );
        assert!(is_guard_hook(g));
    }
    for advisory in [
        "pre-grep",
        "pre-glob",
        "cli-suggest",
        "ceg-observe",
        "post-bash",
        "post-edit",
        "post-write",
        "post-read",
        "session-start",
        "prompt-enhance",
    ] {
        assert!(
            !is_guard_hook(advisory),
            "'{advisory}' must stay an advisory"
        );
    }
}

// ── #4 (04/11/2026): every hook answers JSON or nothing ───────────────────────────
//
// Claude Code applies a hook's stdout fields only when it is a JSON object that passes the
// schema; plain text at exit 0 is "success" and changes nothing (code.claude.com/docs/en/hooks,
// "Exit code output"). Both routes of touring-hook turn prose into the canonical `{}`, so a
// handler that answers prose has its message thrown away on every route. The aprofundamento
// (#4 phase 2) measured 11 such handlers; this sweep is the guard that keeps the set empty.

/// A payload with the fields the hook handlers read, so each one reaches its answer path.
fn sweep_payload(root: &std::path::Path) -> serde_json::Value {
    let file = root.join("src/lib.rs");
    serde_json::json!({
        "session_id": "contract-sweep",
        "transcript_path": root.join("transcript.jsonl"),
        "cwd": root,
        "new_cwd": root,
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "ls", "file_path": file, "pattern": "fn", "path": root},
        "tool_response": {"stdout": "", "stderr": "", "exit_code": 0, "success": true},
        "prompt": "explain the module",
        "message": "note",
        "file_path": file,
        "task_id": "T-sweep",
        "subject": "sweep",
        "trigger": "manual",
        "source": "startup"
    })
}

#[test]
fn every_hook_handler_answers_a_json_object_or_nothing() {
    let mut prose: Vec<String> = Vec::new();
    let mut swept = 0usize;
    for name in all_daemon_hook_names() {
        if is_rpc_endpoint(name) {
            continue;
        }
        let tmp = tempfile::TempDir::new().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("src")).expect("src dir");
        std::fs::write(tmp.path().join("src/lib.rs"), "pub fn f() {}\n").expect("lib.rs");
        let mut rt = crate::runtime::HookRuntime::new(tmp.path()).expect("HookRuntime::new");
        let out = dispatch_table()[name](&mut rt, &sweep_payload(tmp.path()));
        swept += 1;
        let text = out.trim();
        if text.is_empty() {
            continue;
        }
        if !matches!(
            serde_json::from_str::<serde_json::Value>(text),
            Ok(serde_json::Value::Object(_))
        ) {
            prose.push(format!("{name}: {}", truncate_str(text, 120)));
        }
    }
    assert!(swept > 20, "the sweep reached only {swept} hooks — vacuous");
    assert!(
        prose.is_empty(),
        "{} hook handler(s) answer text that is not a JSON object; both routes turn it into \
         `{{}}` and the message is lost. Wrap it (HookResponse::context(..).to_json()):\n{}",
        prose.len(),
        prose.join("\n")
    );
}
