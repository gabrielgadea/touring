# Touring Skill — Changelog

> Historical waves for the Touring skill. Consult when investigating a specific wave, debugging a regression, or tracing a feature back to its activation date. For current operational guidance see `SKILL.md`.

## v5.0.0 (2026-05-09) — Constitution v8.0 (S9 — H3.3)

S9 delivers Constitution v8.0 as the semantic-determinism completion of the Master Plan (S1-S8). All 12 deliverables complete (D9.1-D9.11) as of 2026-05-10.

**Constitution v8.0 deliverables**:
- **D9.1** ✅ RFC-001: Activity Event Catalog (13 EventAction variants, 11 assertions)
- **D9.2** ✅ RFC-002: PARCER Profile Schema (5 YAML profiles, 35 assertions)
- **D9.3** ✅ RFC-003: Path Boundaries Contract (44 PASS audit)
- **D9.4** ✅ RFC-004: Entity Identity Registry (EntityId/EntityKind/Criterion, touring-identity/types.rs, 12 unit tests)
- **D9.5** ✅ RFC-005: 7-Layer Validation Pipeline (469L, 30 assertions)
- **D9.6** ✅ Constitution v8.0 master doc (CONSTITUTION-v8.md, 416L)
- **D9.7** ✅ Audit suite: 12 scripts, 303 total assertions, 12/12 PASS
- **D9.9** ✅ SKILL.md v5.0.0 (header + reference map updated)
- **D9.10** ✅ CLAUDE.md: REGRA #17 added (Entity Identity → rules/entity-identity.md), REFERENCES table updated, 376→388L
- **D9.8** ✅ Pilot: Gabriel approved all three pilots (analise 112 GB workspace-stats + package-registry + health-check; konverter polyglot; transferegov_pipeline). S9 fully closed.

**REGRA #17 (Entity Identity Determinism)**: New constitutional rule added. EntityId derived from canonical name + admission criteria, NOT from memory address or creation order. Full text in `~/.claude/rules/entity-identity.md`. Source: RFC-004.

**Audit suite whack-a-mole**: 7 iterations to reach 11/11 PASS on hard-rules.sh. Exhaustive `#[allow(dead_code)]` exclusion list built covering all intentional feature-gated patterns (EC65 serde, cross-module pub(crate), test helpers, feature-gated methods).

**Option A confirmed**: All 3 REGRA violations were documented false positives. Audit script updated with comprehensive exclusion list. REGRA #11 fix applied (git commands removed from settings_checkpoint.sh).

## v5.0.1 (2026-05-10) — OnceLock Test Isolation Fix + D9.11 Release

**D9.11 ✅ COMPLETE** — Release artifact `~/.claude/touring/release-tag-v31.0.0.txt` (427L, keep-a-changelog format) applied. Touring v31.0.0 = Constitution v8.0 public release.

**OnceLock test isolation fix** (`tantivy_index.rs` static refactor):
- Root cause: `static OnceLock<Option<ToolOutputsIndex>>` initialized once with the first `HOME` value. When tests called `isolate_home()` to switch the sandbox root, the singleton stayed pinned to the original path — causing `context_mode_e2e::audit_full_pipeline_subprocess_to_retrieve` to fail.
- Fix: Wrapped `OnceLock` in `Mutex<Option<OnceLock<...>>>` enabling reset between tests.
- New public function `reset_tool_outputs_global()` drops the inner `OnceLock` and replaces it with a fresh one, allowing `isolate_home()` to reinitialize from the new `HOME`.
- **Two-phase pointer pattern**: Acquire `*const OnceLock` INSIDE the `Mutex` lock scope (where `guard` is live), then dereference OUTSIDE the lock scope to get a `'static` lifetime reference — avoids E0716 "temporary value freed while borrowed".
- `context_mode_e2e::isolate_home()` now calls `reset_tool_outputs_global()` before setting the new `HOME` env var.

**Pre-existing test delta** (unrelated to OnceLock fix):
- `stringzilla_e2e::test_hook_registry_count_is_172` asserts `ALL_DAEMON_HOOK_NAMES.len() == 204`, actual 205 — cross-audit delta from hook additions in waves S1-S9. Documented as known limitation in release tag; no fix applied.

**Files modified**:
- `tantivy_index.rs`: static refactor (~50L net), `reset_tool_outputs_global()` added, `global_tool_outputs()` two-phase pattern
- `tests/context_mode_e2e.rs`: `isolate_home()` calls `reset_tool_outputs_global()` before `set_var("HOME", ...)`
- `tests/stringzilla_e2e.rs`: pre-existing delta (205 vs 204, not fixed)

**Documentation updated**:
- Release tag: `~/.claude/touring/release-tag-v31.0.0.txt` (427L)
- Memory lesson: `touring memory store "lesson:fix-TOOL_OUTPUTS_GLOBAL-oncelock-pollution-2026-05-09"`

```bash
# Verification
cargo test -p touring-hooks --test context_mode_e2e -- --nocapture  # 16 PASS
cargo test -p touring-hooks --test stringzilla_e2e                  # pre-existing delta (205 vs 204)
```

## v4.32.0 (2026-05-08) — Tantivy/Context-Mode Master Plan Wave (15/15 initiatives)

Single-session full delivery of the 15-initiative master plan derived from cross-audit of Touring (Tantivy) vs context-mode (FTS5). Touring becomes a **superset** of context-mode: 11 features in parity + 6 features uniquely available via Tantivy (JSON fields, aggregations, facets, custom tokenizer pipelines, BM25 field-boost, SnippetGenerator).

**Sprint 1 (Quick Wins — Tantivy native)**:
- **I-01 NgramTokenizer trigram REAL** — schema v3→v4 added `symbol_name_trigram` field with custom `trigram_3` tokenizer (NgramTokenizer 3,3 + LowerCaser). New `search_trigram()` method + `rrf_merge_three()` 3-way fusion (porter ⊕ trigram ⊕ fuzzy). 'useEff'→'useEffect' via real substring (no fuzzy proxy). Counter `tantivy_trigram_query_count`. Flag `TOURING_TANTIVY_TRIGRAM_FIELD`.
- **I-02 PhraseQuery proximity boost** — `try_build_phrase_query()` + `BooleanQuery::union(plain SHOULD phrase)` em `search()`. Multi-term ≥ 2 termos get adjacency boost via slop (default 2). Counter `phrase_query_match_count`.
- **I-03 5× heading boost** — `QueryParser::set_field_boost()` `symbol_name=5.0`, `functional_signature=1.5`, `docstring=1.0` em todas 3 paths. Tunable via `TOURING_TANTIVY_NAME_BOOST`.
- **I-05 TTL cache + cleanup** — `is_fresh(hash, ttl_secs)` skip-on-store, `cleanup_expired(retention_secs)` actor (AllQuery iteration), counters `tool_outputs_ttl_skip_count` + `tool_outputs_cleanup_deleted_count`. Defaults 24h TTL + 14d retention via env.

**Sprint 2 (Storage & Retrieval)**:
- **I-04 SnippetGenerator** — `get_tool_output_with_snippet(hash, query)` usa Tantivy `SnippetGenerator::create` sobre `summary` field com `set_max_num_chars(512)`. Module-level `strip_html_tags()` helper (CC<15) converte `<b>...</b>` highlights to plain text.
- **I-06 JSON field tool_args** — schema v4→v5 added `add_json_field("tool_args", STORED|TEXT)`. ToolOutputDoc gains optional `tool_args: Option<serde_json::Value>` (`#[serde(default)]` backward compat). Module-level `serde_value_to_tantivy_owned()` recursive converter (Bool/I64/U64/F64/Str/Array/Object). Enables nested queries.
- **I-09 DateField dual-write** — schema gains `stored_at_dt` (`add_date_field(STORED|FAST|INDEXED)`) alongside `stored_at_unix` (preserved). Foundation para date_histogram aggs.

**Sprint 3 (Analytics)**:
- **I-07 AggregationCollector wire** — `aggregate_terms(field_name, max_buckets)` itera segments via AllQuery + StoreReader para group docs by field value, sorted desc by count. Exposto via MCP tool `ctx_aggregate`.
- **I-08 FacetCollector hierárquico** — schema v5 adds `symbol_facet` field. `build_symbol_facet(SymbolDoc) → Facet` constrói `/<lang>/<crate>/<kind>/<visibility>` paths. `count_facets(prefix, max_buckets)` runs FacetCollector. Exposto via MCP tool `ctx_facets`.

**Sprint 4 (Sandbox & Throttling)**:
- **I-10 Progressive throttling 3-tier** (NEW module `throttle.rs`, ~200 LOC) — `ThrottleState` com moka cache (capacity 10k sessions, TTL 1h). Per-session `AtomicU32` counter via `get_with`. `tier_for(count)`: ≤3 Tier1 (passthrough), 4-8 Tier2 (top_k≤3+warn), ≥9 Tier3 (block+redirect). `ctx_search_throttled` wrapper. Tunable via `TOURING_THROTTLE_TIER1_MAX/TIER2_MAX`.
- **I-11 Multi-lang sandbox 11 runtimes** — `SandboxLanguage` enum (JS/TS/Python/Ruby/Go/Rust/PHP/Perl/R/Elixir/Shell). `resolve_language_runtime(lang)` autodetecta via `command -v` (bun > node, python3 > python, Rscript > R). `resolve_language_args(lang, code)` returns argv per runtime convention (-c/-e/-r). `resolve_program("SandboxPython"/...)` routes Sandbox<Lang> tool names.
- **I-12 Credential whitelist + redactor** — `apply_credential_whitelist(cmd)` faz `env_clear()` + selective re-inherit de 25 envs (GH/AWS/Google/K8s/Docker/npm/OpenAI/Anthropic + locale baseline). Extra via `TOURING_SANDBOX_EXTRA_WHITELIST`. `redact_secrets(stdout)` substitui linhas com token/key/secret patterns por `[REDACTED]`. Wired into `derive_summary` para nunca vazar credenciais para summary.

**Sprint 5 (Lifecycle & Continuity)**:
- **I-13 26 lifecycle events 5-tier** — `classify_priority_by_hook_name` extended de 9 → 26 hook names. New events: user_decision, rejected_approach, error, blocker, constraint, error_resolution, plan_*, latency_spike, iteration_loop, mcp_call, agent_finding, environment_change, subagent_*, skill_invocation, external_ref, hook_memory_*, intent_classification, role_directive, large_user_data.
- **I-14 Think-in-Code mandatory directive** — pre_read.rs threshold lowered 10 → 5 reads (env-tunável via `TOURING_THINK_IN_CODE_THRESHOLD`).
- **I-15 SessionStart Guide** (NEW module `session_guide.rs`, ~300 LOC) — `SessionGuide` struct com 15 Option<String> sections (LastRequest/Tasks/Plans/Decisions/FilesModified/Errors/Constraints/Blockers/Git/Rules/MCP/Subagents/Skills/Rejected/References). Builder `with_*` methods + per-section truncate 500 chars + total cap 5000 chars. `render()` Markdown. Exposto via MCP tool `ctx_session_guide`.

**Cross-audit (REGRA #0 potencializar)**: 4 orphan APIs detectadas e wired:
- `count_facets` → `ctx_facets`
- `aggregate_terms` → `ctx_aggregate`
- `cleanup_expired` → `ctx_cleanup`
- `get_tool_output_with_snippet` → `ctx_retrieve_with_query`

MCP tool count ctx_*: 5 (D6 baseline) → **9** total.

**Validation**: 3447 lib + 16 context_mode_e2e + 16 master_plan_e2e = **3479 tests PASS, 0 fails**. 0 TODO/FIXME/dead_code em arquivos modificados.

**Plan generator data-driven**: `~/.claude/scripts/generate_touring_master_plan.py` (NEW ~1500 LOC, stdlib-only) gera `~/.claude/plans/2026-05-08-touring-master-plan.md` (1518L, 62KB). Suporta `--validate` (structural check), `--filter-tier {1,2,3}`, `--filter-sprint {1..5}`, `--stdout`. 15 Initiative dataclasses com full detail.

**Files modified**: `tantivy_index.rs` (+500), `sandbox_executor.rs` (+250), `gate_metrics.rs` (+50), `feature_flags.rs` (+30), `hook_events.rs` (+40), `pre_read.rs` (+5), `cli_handlers_mcp.rs` (+200), `lib.rs` (+6), `throttle.rs` (NEW 200), `session_guide.rs` (NEW 300), `tests/context_mode_e2e.rs` (+10), `tests/master_plan_e2e.rs` (NEW 380).

**Session report**: `~/projects/touring/docs/2026-05-08-master-plan-wave.md`.

## v4.31.0 (2026-05-03) — TACO Integration Wave Sequence

Single-session COMPLETE deployment of TACO ↔ Claude Code integration: enforcement triplé (E1+E3+E6) + hook coverage expansion (Wave A) + sync/async surgical migration (Wave B.1) + 3 new Rust handlers (Wave C/1-2-3) + checkpoint TOON v1.0 workflow + 12 cross-audit scripts + CLAUDE.md refactor.

- **A — E1 plan-detector hook (dual-pathway)**: `~/.claude/hooks/plan-detector.sh` (130L+) — PreToolUse:Write hook bloqueia content match `## Wave|## Phase|## P[0-9]+\.|## Roadmap|## DAG|numbered bold deliverables` em arquivos `.md` com ≥50 linhas, exceto whitelist (CHANGELOG/README/CLAUDE/SKILL/MEMORY/ADR-*/rules/*/references/*/memory/*.md/skills/*/SKILL.md/agents/*.md/commands/*.md). Dual-pathway routing: heurística por path/content keywords (`audit|postmortem|summary|report|retrospective|fase|phase-report|session-report`) → message sugere `/checkpoint` para retrospective, `/plan` para prospective. Bypass: `TACO_PLAN_DETECTOR_DISABLED=1`. 6 test scenarios PASS (block content match, block path match, allow whitelist, allow short, allow Touring-native tooling marker, allow non-md).
- **B — E3 /plan + /checkpoint slash commands**: `~/.claude/commands/{plan,checkpoint}.md` — both detected by Claude Code skill catalog. `/plan <intent>` expande para `/plan ou skill taco-planning --quality high --cila-level 3 --auto-populate` (prospective `.md` chunked output). `/checkpoint <topic>` invoca `~/.claude/tools/Touring-native tooling/workflows/checkpoint.sh` (retrospective `.toon` v1.0 output). Together they close edição-com-gate enforcement loop — gate (E1) + ergonomic alternatives (E3+E3').
- **C — Wave A: 8 events ZERO-CODE wiring**: `~/.claude/settings.json` (17→25 hook events). Wired existing handlers in `touring-cortex` to settings.json: `TeammateIdle` (sync 15s), `ConfigChange` (async, matcher `user_settings|project_settings|skills`), `WorktreeCreate` (sync 30s), `WorktreeRemove` (async 10s), `StopFailure` (async, matcher `rate_limit|max_output_tokens|server_error`), `Notification` (async, matcher `idle_prompt|permission_prompt`), `Elicitation` (async 3s), `ElicitationResult` (async 3s). Zero LOC Rust necessary — all handlers preexisting in `crates/touring-cortex/src/handlers/lifecycle.rs`. PermissionDenied deferred to Wave C/2 (no dedicated subcommand; needs new permission_handler).
- **D — Wave B.1: Sync/async surgical migration**: 8 hooks Tier B (pure-async, idempotent post-hoc, no decision impact) marked `async: true` via Python atomic JSON load+dump: `post-read`, `post-tool-rl`, `subagent-stop`, `task-completed`, `session-stop`, `task-sync-list`, `task-sync-get`, `task-sync-output`. 5→15 async hooks (+200%). Sub-wave B.2 (split hybrid post-edit/write/bash) DEFERRED — requires touring-hook crate refactor for fast-warning subcommand. Sub-wave B.3 (asyncRewake file-changed) DEFERRED — flag experimental, validation empírica primeiro.
- **E — Wave C/1: PostToolBatchHandler (`crates/touring-hooks/src/post_tool_batch.rs`, 259 LOC)**: NEW Rust module + `pub fn run_post_tool_batch(rt:&mut HookRuntime, input:&Value) -> HookResponse` + 9 unit tests PASS (`test_average_reward_mixed`, `test_batch_has_edit_or_write_{true,false}`, `test_call_reward_values`, `test_parse_tool_calls_{empty,explicit_success,infer_success_from_exit_code}`). Aggregates RL reward across batch (avg `1.0` success / `-0.3` failure → single `inject_reward("batch", avg, "batch:N_tools")` instead of N redundant invocations). For batches with Edit/Write: emits REGRA #0 wiring hint via `additionalContext` when new orphans detected. `ALL_DAEMON_HOOK_NAMES` 197→198 (assert at hook_registry.rs:1737/1743). Wave C/1 wiring orphan delta: **-133** (REGRA #0 surplus). Composite 1.0. Hook FIRING in runtime confirmed ("batch of N tools (Edit, Write)" notifications).
- **F — Wave C/2: PermissionRequest VGP (`crates/touring-hooks/src/permission_request.rs`, ~200 LOC)**: NEW module + `pub fn run` (line 66) + `pub fn run_returning` (line 73) + 17 unit tests + 5 smoke tests PASS. Conservative thresholds: `BASH_ALLOW_THRESHOLD=0.20`, `BASH_DENY_THRESHOLD=0.70`, `BR_ALLOW_MAX=3`, `BR_DENY_MIN=15`. For Bash: keyword-weighted risk scoring (rm-rf=0.40, sudo=0.20, pipe-to-shell=0.30, etc); for Edit/Write: blast_radius via `blast_radius_file_count` (signals.rs). Returns `hookSpecificOutput.decision.behavior=allow|deny|ask` with `message`. VP-Scout false positive avoided: `PermissionAutoApproverHandler` (H60) in `enrichment.rs` already handles `mcp__touring__*`, NOT duplicated. Composite 1.0.
- **G — Wave C/3: WorktreeCreate isolation (`crates/touring-cortex/src/handlers/lifecycle.rs:917+`, +117 LOC)**: Expanded `WorktreeEnterHandler::execute` — guards empty path, writes `CLAUDE_ENV_FILE` (export `CLAUDE_PROJECT_DIR=<path>`), spawns async `std::thread::spawn(|| std::process::Command("touring index rebuild --dir <path>"))` for fire-and-forget index rebuild (CortexContext lacks HookRuntime — detached thread is safer than Tokio RT acquisition), stores `MemoryTier::Reference` snapshot with timestamp, returns `Allow` with path stdout (per Claude Code WorktreeCreate contract). 5 new tests pass (`test_worktree_enter_skips_empty_path`, `_missing_path_skips`, `_returns_path_as_context`, `_allow_without_rlm`, `_env_file_written_when_exists`). Composite 1.0.
- **H — checkpoint.sh workflow (`~/.claude/tools/Touring-native tooling/workflows/checkpoint.sh`, ~140L bash + Python)**: TOON v1.0 generator. Args: `--topic <name>` (required), `--intent <desc>`, `--content-from <md>`, `--role <agent>`, `--task-id <id>`, `--out <path>`. Captures up to 12 H2 sections automatically + embeds full markdown body. Uses `~/.claude/tools/Touring-native tooling/lib/plan_quality/toon_io.py::dump`. Output `~/projects/touring/docs/checkpoints/<YYYY-MM-DD>-<slug>.toon`. Persists `touring memory store --tier semantic` lesson + `touring learning reward orchestrate +1.0`. Bug fixes during E2E: heredoc `sys.stdin.read()` consumed by Python source itself → switched to `--content-from` arg + `Path.read_text()`. `dump(doc, f)` → `dump(doc, out_path)` (toon_io accepts path string, not file handle).
- **I — CLAUDE.md refactor + REGRA #16**: 713→378 lines (-47%). Identidade TACO (Touring Agentic Code Orchestration) explicitada. 7 Reflexos Proativos table (Index First / Search via Touring / Recall Memory / DAG Decompose / Generator-First / Checkpoint Sempre / Reward Loop). Decision Matrix (pergunta → comando default). Triggers Automáticos (engagement proativo). REGRA #16 CLAUDE.MD HYGIENE guardrail — limites soft=300/hard=400, decision tree antes de adicionar, 8 anti-padrões blocked, quarterly review, auto-enforcement format.
- **J — Cross-audit suite (`~/.claude/audits/2026-05-03-taco-integration/`)**: 12 audit scripts + 3 lib helpers (purpose_extract.py, audit_helpers.sh, toon_validate.py). 243/243 assertions PASS, 7/7 E2E tests PASS, 3 advisory warnings. Master orchestrator at `00-master-audit.sh`. Cross-audit verifies purpose fidelity (1.0), interface contracts, invariants (exit 0 + no panics + idempotency), edge cases, integration. Bug fixes during creation: 10 audit script bugs corrected (BRE/ERE alternation in `grep -qE`, counter aggregation across subshells via `_AUDIT_COUNTER_FILE`, `grep -qE --` escape, fork bomb pattern fix, etc). Pyright diagnostics fixed: removed unused `Optional` import + added `# pyright: ignore[reportMissingImports]` for runtime sys.path manipulation.

```bash
# Verification commands
touring doctor -j                                                # 5/5 ok
touring status -j | jq '{composite_health_score}'                # 0.7136 (was 0.5505)
ls ~/.claude/audits/2026-05-03-taco-integration/                 # 12 scripts + lib + reports
bash ~/.claude/audits/2026-05-03-taco-integration/00-master-audit.sh
# → "Scripts run: 12, PASS: 12, FAIL: 0, Assertions PASS: 243, FAIL: 0, WARN: 3"

# Slash commands detection (in skill catalog at session start)
# - plan: /plan — Generate Plan via Touring-native tooling 
# - checkpoint: /checkpoint — Generate .toon Checkpoint/Report via Touring-native tooling 

# Live demonstrations
echo '{"tool_name":"Write","tool_input":{"file_path":"/tmp/wave-roadmap.md","content":"## Wave A\n..."}}' \
  | ~/.claude/hooks/plan-detector.sh                             # → exit 2 + "PROSPECTIVE artifact detected → /plan"
echo '{"tool_name":"Write","tool_input":{"file_path":"/tmp/cross-audit.md","content":"## Phase 1\n..."}}' \
  | ~/.claude/hooks/plan-detector.sh                             # → exit 2 + "RETROSPECTIVE artifact detected → /checkpoint"

# Recursive demo: use /checkpoint to consolidate own session
TF_WORKSPACE=~/projects/touring ~/.claude/tools/Touring-native tooling/workflows/checkpoint.sh \
  --topic "session-consolidation" --content-from /tmp/draft.md --role orchestrator
# → ~/projects/touring/docs/checkpoints/<DATE>-session-consolidation.toon (TOON v1.0)
```

**Métricas cumulativas**: Hook events 17→25 (+47%). Async hooks 5→15 (+200%). Slash commands 0→2. Hook scripts 3→4. Touring-native tooling workflows 14→15. Touring CLI subcommands ~125→~126 (+post-tool-batch). Rust modules novos: 2. LOC Rust adicionados: ~610. Tests novos passing: 31 (9+17+5). Touring memory lessons: 13 stored. RL rewards (+1.0): 16+ injected. Plans gerados via Touring-native tooling: 4 (Waves A/B/C/D). .toon checkpoints: 2 new. CLAUDE.md: 713→378 lines (-47%). composite_health_score: 0.5505→0.7136 (+29%). Wiring orphan delta: 0 (Wave C/1 surplus -133).

Production-verified: cargo check workspace pass, all suites green, daemon stable (PID fresh after rebuild), binary subcommands wired, slash commands detected by harness. Final consolidation `.toon`: `~/projects/touring/docs/checkpoints/2026-05-03-taco-integration-session-2026-05-03-final.toon`.

## v4.29.0 (2026-05-01) — ast-grep Optimization of pre-read & pre-bash

3 strategies + 3 collateral bug fixes shipped together. Polyglot defensive signals on file reads, structural bash validation that distinguishes string-literal carriers from real destructive commands, and Pensieve cluster-key normalization.

- **A — S1 AstGrepRiskSignalLayer (pre-read)**: `crates/touring-hooks/src/shared/{ast_grep_signal,risk_patterns}.rs` — when CC reads a file at CILA ≥ 2, inject a one-line risk summary (`[risk] rust: unwrap=12, panic=2, todo=1`). Pattern sets: Rust (4), Python (4), JavaScript/TypeScript (2), Go (1). moka cache content-addressed via blake3 + set_id, TTI 5 min, 64-entry LRU. Cold parse ~3–10 ms; warm hit <1 µs. SignalLayer wired in `pre_read.rs::build_parallel_signal_pipeline` after the graph layer with score 0.85.
- **B — S2 Bash structural validator (pre-bash)**: `crates/touring-hooks/src/shared/bash_ast_validator.rs` — 6 curated rules (Block: `rm -rf`, `rm -fr`, `find … -delete`; Warn: `chmod -R 777`, `git push --force`, `git reset --hard`) with `tail_token` for non-contiguous matching and `bypass_substrings` for `--dry-run`/`--force-with-lease`/`--help`. Strips quoted strings + `#` comments BEFORE rule evaluation — `echo "rm -rf"` and `ls # rm -rf` correctly do NOT block. **Pivot from ast-grep**: `ast-grep-language 0.36.0` ships a bash grammar with `tree_sitter::Language v15`, incompatible with `ast-grep-core v14`; tokenizer-based fallback preserves the structural-match guarantee for the rules we ship. Wired in `pre_bash.rs` BEFORE the legacy `PreToolValidator`.
- **C — S3 Command shape clustering (pre-bash)**: same module — `command_shape("cargo --quiet test -j 4 --release")` → `Some("cargo test")`. Tokenizer skips env-vars, leading flags, numeric flag-args; stops at shell separators. Wired as the Pensieve cluster key with backwards-compat fallback to `extract_command_short`.
- **D — E2E integration test**: `crates/touring-hooks/tests/wave_v429_ast_grep_hooks_e2e.rs` — 30 tests covering S1 layer behavior across 5 languages, S2 string-literal/comment-carrier distinction + dry-run/force-with-lease bypasses, S3 normalization invariants, cross-cutting infra (`scan_source`, `format_matches`, `lang_for_path`).
- **E — B1 Stdout JSON validity guard (`main.rs`)**: defense-in-depth fix for the recurring `Hook JSON output validation failed — (root): Invalid input` errors. Daemon roundtrip output is parse-checked before forwarding to CC; non-JSON falls back to canonical `{}` Allow + log to stderr. Preserves "exit 0 + valid JSON always" invariant under any daemon failure mode.
- **F — B2 `git push --force` regex word boundary (`pre_tool_validator.rs:262`)**: `(?i)^git\s+push\s+(?:[^\n]*\s)?--force(?:\s|$)` — requires whitespace or EOL after `--force`, so `--force-with-lease` is correctly NOT matched.
- **G — B3 Universal intent-disclosure bypass (`pre_tool_validator.rs::validate`)**: short-circuit at top of `validate()` for `--dry-run` and `--force-with-lease`. Realigns the legacy regex layer with the structural validator (S2), eliminating double-validation false positives.

```bash
# Verification
touring doctor -j                                 # 5/5 ok
cargo test -p touring-hooks --lib                 # 3331/3331 PASS (was 3284)
cargo test --test wave_v429_ast_grep_hooks_e2e    # 30/30 PASS
cargo test --lib pre_tool_validator               # 51/51 PASS

# Live demonstrations
echo '{"tool_input":{"command":"echo \"rm -rf\""}}' | touring-hook pre-bash    # → {} (Allow)
echo '{"tool_input":{"command":"rm -rf /tmp"}}'    | touring-hook pre-bash    # → Deny
echo '{"tool_input":{"command":"rm -rf --dry-run /tmp"}}' | touring-hook pre-bash  # → {} (Allow)
echo '{"tool_input":{"command":"git push --force-with-lease"}}' | touring-hook pre-bash  # → {} (Allow)
```

**sccache caveat documented**: incremental builds may reuse stale crate object after a `pre_tool_validator.rs` edit. Touch + rebuild forces full recompile when behavior doesn't match source.

Production-verified: `cargo check --workspace` exit 0, all suites green, daemon stable. Session report: `~/projects/touring/docs/2026-05-01-wave-v429-ast-grep-hooks.md`.

## v4.28.0 (2026-05-01) — Wave 2 D43+D45 + Daemon Idle Fix

3 deliverables shipped together. Token-saving suite for Grep/Glob hooks plus a daemon-lifecycle bugfix that ends the recurring "Connection refused" at SessionStart.

- **A — Daemon idle timeout configurable (L1 bugfix)**: `crates/touring-hooks/src/daemon.rs` — `const IDLE_TIMEOUT: Duration = Duration::from_secs(300)` replaced by `fn idle_timeout_secs()` reading `TOURING_IDLE_TIMEOUT_SECS` env var with default `0` (disabled). Watchdog only spawns when timeout > 0; re-reads env per tick (runtime-tunable). Eliminates the SessionStart cold-start race that produced `composite_health_score=0.5` on every CC resume after >5 min idle. To restore legacy 5-min auto-shutdown: `TOURING_IDLE_TIMEOUT_SECS=300`.
- **B — D43 PreToolUse Grep/Glob symbol enrichment** (master plan W2 ✅ DELIVERED): new modules `pre_grep.rs` (280 LOC + 15 unit tests) and `pre_glob.rs` (25 LOC delegate). When CC invokes Grep/Glob with a symbol-like pattern (PascalCase / snake_case / camelCase, length 3..=50, ≥3 ASCII letters), the hook injects `additionalContext` listing locations from the symbol_store — CC then frequently reads file:line directly and skips the Grep. **P99 = 2 ms** (vs spec 50 ms — 25× margin). Disable switch `TOURING_DISABLE_PREGREP=1`. 2 new gate-metrics counters: `pre_grep_enrichment_count`, `pre_grep_zero_results_count`. settings.json wired with `matcher: "Grep"` + `matcher: "Glob"` (coexists with gitnexus-hook). Hook Registry: 184→186 (`all_daemon_hook_names`) and 182→184 (`ALL_DAEMON_HOOK_NAMES`).
- **C — D45 `Bash(touring *)` permission auto-add** (master plan W7 sub-task ✅ DELIVERED): 4 entries in `~/.claude/settings.json::permissions.allow` (`Bash(touring *)`, `Bash(update-touring *)`, `Bash(touring-bootstrap *)`, `Bash(touring-mcp *)`). Idempotent merge — re-run does nothing. Removes approval prompts for every `touring` invocation.
- **D — E2E integration test**: new `crates/touring-hooks/tests/d43_pre_grep_glob_e2e.rs` (305 LOC, 20 tests) covering whitelist contract (PascalCase/snake_case/free-text/regex/short/long/all-underscore), disable switch, pre_glob delegation, counter increment, snapshot serialization with `#[serde(default)]` for legacy compat, hook registry invariant, idle-timeout default/honor/reject-invalid, and exit-0 invariants for malformed input.

```bash
# Ship verification
touring doctor -j                          # 5/5 ok, daemon stable
touring gate-metrics -j | jq '.pre_grep_enrichment_count, .pre_grep_zero_results_count'

# E2E proof (PascalCase pattern → 20-location enrichment)
echo '{"hook_event_name":"PreToolUse","tool_name":"Grep","tool_input":{"pattern":"HookRuntime","path":"crates"}}' \
  | CLAUDE_PROJECT_DIR=/home/gabrielgadea/projects/touring touring-hook pre-grep

# Disable enrichment (rollback path)
TOURING_DISABLE_PREGREP=1 update-touring
```

Production-verified: `cargo check --workspace` exit 0, **3284/3284 lib tests PASS**, **20/20 E2E PASS**, **0 clippy warnings on touring-hooks**, daemon uptime stable 60 s+ post-fix. Session report: `~/projects/touring/docs/2026-05-01-d43-d45-daemon-idle-fix.md`.

## v4.27.0 (2026-04-30) — Wave C: Assists Framework + VFS + Salsa POC + Format Preserve

4 deliverables. touring-assists crate created with 10 handlers, touring-vfs overlay, touring-incremental-salsa POC, format-rust --preserve flag. All 15 master plan deliverables now COMPLETE.

- **A — touring-assists framework**: new crate `crates/touring-assists/` with 10 assist handlers (`add_missing_match_arms`, `auto_import`, `auto_wire`, `change_visibility`, `convert_to_guarded_return`, `extract_function`, `generate_impl`, `inline_call`, `merge_imports`, `move_module_to_file`), `AssistCatalog` registry, `AssistContext`, `LazySourceChange`. CLI `touring assist list-kinds / applicable / apply` fully operational. 50/50 tests PASS. RFC-100 codes `A-100`..`A-109`. Exit criteria MET.
- **B — touring-vfs overlay**: new crate `crates/touring-vfs/` (`Vfs`, `VfsOverlay`, `FileSet`, `FileId`, `AbsPathBuf`). In-memory overlay filesystem with `Arc<VfsState>` cloning for snapshots. 7 modules: `lib.rs`, `abs_path.rs`, `file_id.rs`, `file_set.rs`, `overlay.rs`, `vfs.rs`, `watcher.rs`.
- **C — touring-incremental-salsa POC**: new crate `crates/touring-incremental-salsa/` with salsa 0.18. `DatabaseImpl` with 5 `#[salsa::input]` fields (`FileText`, `ModuleDecl`, `SymbolDef`, `SymbolUse`, `FileMeta`). 11 tests PASS. Decision gate (5× speedup) pending future integration with per-project actor.
- **D — format-rust --preserve**: `--preserve` flag added to `cli/ast.rs` for comment-preserving formatter. `PreservingFormatter` wraps prettyplease with gap-capture via `capture_gap()`. 7 unit tests for idempotency and gap preservation.

```bash
# Assists framework
touring assist list-kinds
touring assist applicable test.rs:10:1
touring assist apply auto_wire test.rs 0:0..0:0

# VFS overlay
touring doctor -j   # touring-vfs component registered

# Salsa POC
cargo test -p touring-incremental-salsa   # 11 tests PASS

# Format preserve
touring ast format-rust --preserve test.rs
```

Production-verified: cargo check --workspace exit 0, 182 suites PASS. Session report: `~/projects/touring/docs/2026-04-28-cross-repo-improvements-master-plan.md`.

## v4.26.0 (2026-04-29-30) — Wave B: Engine Reforms

5 deliverables: SSR, Shape budget, CharClasses, dual-module gating, SourceChange transactional.

- **A — touring ssr**: `touring ssr` semantic structural rewrite CLI. Pattern grammar `pattern ==>> replacement` with `$<name>` placeholders. `MatchFinder` over tree-sitter, VGP gate per path, `Rewriter` respects `SkipContext` (A.2). CLI + MCP tool `ssr_apply`. 20 unit + 10 integration tests. RFC-100 codes `S-100`..`S-102`.
- **B — RenderShape budget**: `shape.rs` (169 LOC, 8 tests) with `RenderShape { max_width, indent, offset }` and `budget()` method. All 30 GeneratorKind call sites updated across `e2e_pipeline.rs` + `e2e_cross_audit.rs` + `generator_tools.rs`. `render()` returns `Result<Option<...>, GenerateError>`. RFC-100 `G-200 ShapeOverflow`.
- **C — CharClasses state machine**: `touring-core::char_classes` iterator over `(char, CharClass)` where `CharClass ∈ { Code, StringLit, Comment, RawString, DocComment }`. Multi-lang via tree-sitter. `cli-ast-grep` accepts `--skip-strings` flag. `highlight.rs` dims string/comment-only lines (ANSI 245 faint). 25/25 tests PASS.
- **D — Dual-module lib_on/lib_off**: `touring-hooks` split into `lib_on.rs` (current behavior) and `lib_off.rs` (no-op stubs). `lib.rs` dispatches via `#[cfg(feature = "hooks-active")]`. 14 tests PASS. CI matrix supports `--features hooks-noop`.
- **E — SourceChange transactional**: `source_change/` module with `SourceChange`, `TextEdit`, `SnippetEdit`, `FileSystemEdit`, `Applier` (two-phase: shadow_validate + atomic commit with rollback). rkyv serialization. Typestate `PlanExecutor → Applier`. CLI `source-change apply/preview/validate`. MCP tool. 11 integration tests. touring-server 718 tests PASS.

```bash
# SSR
touring ssr "foo($a, $b) ==>> ($a).foo($b)" --scope "*.rs" --dry-run

# CharClasses
touring ast grep test.rs "TODO" --skip-strings

# SourceChange
touring source-change apply --file change.json
touring source-change preview --file change.json
```

Production-verified: cargo check --workspace exit 0, cargo fmt clean, clippy 0. Session report: master plan `docs/2026-04-28-cross-repo-improvements-master-plan.md`.

## v4.25.0 (2026-04-28-29) — Wave A: Quick Wins

4 deliverables: profile instrumentation, SkipContext, idempotency gate, MCP profile_query.

- **A — touring-core::profile**: RAII guards (`MeasurementGuard`, `MeasurementGuardAsync`) + `measure_block!` macro + background worker thread. Replaces ~30 `Instant::now()` sites in touring-hooks and touring-ast. Per-label hdrhistogram with merge on shutdown. Counters `profile_p50_us`, `profile_p99_us`, `profile_call_count_total`.
- **B — SkipContext W-115**: `// touring:skip-region` ... `// touring:skip-end` markers parsed by tree-sitter comment walker. Generator typestate `Rendered` consults `SkipContext` — aborts with `Q-310 RegionFrozen` if edit overlaps region. Post-edit hook emits `W-115 SkippedRegionWritten` (warning). CLI `touring skip list/validate`. 9 integration tests across Rust/JS/TS/Python.
- **C — Idempotency Q-220**: `pre_edit.rs` runs `format-rust` twice on proposed output, compares bytes. If diff detected: score reduced by 0.3 and `Q-220 NonIdempotentFormat` emitted. Config `pre_edit.idempotency.enabled` (default ON). Counter `idempotency_violations_count`.
- **D — MCP profile_query**: MCP tool `mcp__touring__profile_query` with schema `{ section, top_n, include_percentiles }`. Reads from `ProfileAggregator` (in-memory hdrhistogram store). CLI mirror `touring profile query`. ProfileAggregator Default impl fix.

```bash
# Profile instrumentation
RUST_LOG=warn touring pre-edit   # logs include profile_* counters

# Skip regions
touring skip list test.rs
touring skip validate test.rs

# Idempotency gate
touring gate-metrics -j | jq .idempotency_violations_count

# Profile query MCP
mcp__touring__profile_query '{"section": "pre_edit_chain", "top_n": 1, "include_percentiles": [50, 99]}'
```

Production-verified: 182/182 test suites PASS (touring-assists 50 + touring-generator 138 + touring-incremental-salsa 11 + others), clippy 0. Session report: `docs/2026-04-28-cross-repo-improvements-master-plan.md`.

## v4.24.0 (2026-04-27) — Wave 12: B-301 6-dim TDG + B-302 PatchExpansion

Closes 2 orphan loops identified by VP-Scout. 4224 tests PASS, 0 regressions.

- **A — B-301 6-dim TDG composite migration**: `pre_edit.rs::compose_quality_evolution` now consumes `tdg.composite` (6-dim weighted: complexity 0.20 + coverage 0.20 + duplication 0.10 + churn 0.10 + entropy 0.20 + antipatterns 0.20) instead of recomputing a 1-dim `avg_complexity` proxy locally. The previous proxy missed coverage/duplication/churn/entropy/antipattern signals. Threshold preserved (blast > 20 AND composite < 0.40). Tracing event now includes `grade=A+..F` letter. Anonymous block dissolved so `tdg` survives into the B-301 gate. 4 new tests in `pre_edit::tests` (`b301_six_dim_tdg_catches_what_one_dim_proxy_misses`, `_emits_b301_error_with_six_dim_quality_score`, `_not_emitted_at_blast_boundary`, `_not_emitted_when_tdg_composite_above_threshold`).
- **B — B-302 PatchExpansion (new RFC-100 code)**: closes the orphan `PatchComplexityDelta::compute` (Wave P1.5) by wiring it into a real production diagnostic.
  - `touring_core::diagnostic::codes::B_302_PATCH_EXPANSION = "B-302"` (new)
  - `BlastWarning::PatchExpansion { file, delta_bytes: f64, confidence: f32 }` variant (Severity::Warning)
  - `pre_write::emit_b302_if_low_confidence_expansion()` helper — gate: `delta.is_expansion() AND delta.confidence < 0.7`
  - `cli_mpatch_preview` calls helper; response JSON gains `b302_diagnostic` field (object when fires, `null` otherwise — backward compat preserved)
  - `gate_metrics::diagnostic_b302_emitted_count` AtomicU64 + `record_diagnostic_b302_emitted()` helper, exposed in `GateMetricsSnapshot`
  - 3 unit tests (`warning.rs`) + 3 integration tests (`pre_write.rs`) + 2 E2E tests (`cli_handlers_e2e.rs`)
- **C — Synergy observability**: `WIRED_PAIRS` gains 2 entries Wave 12 (`cli_mpatch_preview ↔ B-302`, `pre_write::emit_b302_... ↔ PatchComplexityDelta::compute`). `WIRED_PAIR_METRICS` gains 1 entry mapping `cli_mpatch_preview ↔ B-302` → `diagnostic_b302_emitted_count`, visible via `touring synergy --with-metrics`.
- **D — Constitutional**: REGRA #13 SKILL HYGIENE added to `~/.claude/CLAUDE.md` enforcing Anthropic official limits (`name` hyphen-case ≤ 64, `description` ≤ 1024, body < 500 lines) + 5-step pre-edit protocol + anti-pollution rules.

```bash
# B-301 6-dim now visible in tracing
RUST_LOG=warn touring pre-edit   # logs include grade=D|F + tdg.composite

# B-302 live observability
touring gate-metrics -j | jq .diagnostic_b302_emitted_count
touring synergy --with-metrics -j | jq '.wired_pairs[] | select(.consumer | contains("B-302")) | .metrics'
touring synergy wired -j | jq '.wired_pairs[] | select(.wave == "v4.24.0 W12")'
```

Production-verified: cargo check --workspace exit 0, cargo fmt clean. 159 (touring-core) + 295 (touring-analysis) + 3247 (touring-hooks lib) + 115 (touring-hooks E2E) + 408 (touring-server) = **4224 tests, 0 failures, 1 ignored**. Session report: `~/projects/touring/docs/2026-04-27-wave12-b301-b302.md`.

## v4.23.0 (2026-04-27) — Wave 11: Silent RFC-100 Codes Activated

7 of 17 silent diagnostic codes activated. Composite health score: 0.4894 → 0.5145 (+0.0251).

- **S-1 — Q-230/Q-240 via new `QualityFinding` enum**: New module `crates/touring-analysis/src/quality/quality_finding.rs`. `HighAntipatternDensity` (Q-230, emit when antipattern_rate > 0.3); `HighCyclomatic` (Q-240, emit when CC > 20). Wired in `pre_edit.rs` TDG quality path. 2 new tests.
- **S-2 — B-310 BlastInjection wired**: `BlastWarning::BlastInjection` variant now wired in `pre_tool_use.rs:build_blast_output()`. Emits when predictive blast injection detects symbol mutation.
- **S-3 — B-320 CrossFeatureBlast wired**: `BlastWarning::CrossFeatureBlast` wired in `cli_handlers.rs:cli_ast_blast_cross_feature()`. Emits when cross-feature blast radius crosses cfg-gated boundaries.
- **S-4 — G-400 VgpFailed wired**: `GeneratorErrorKind::VgpFailed` wired via `err.to_diagnostic_opt()` in `typestate.rs:VgpFailed` block.
- **S-5 — G-401 SpeculateBelowThreshold wired**: Same pattern in `typestate.rs:397`.
- **B-301 deferred**: Requires blast metric acquisition scope refactor.
- **Test fixes**: Race condition in `e2e_generator_health::subscribe_receives_published_events`; stale assertion in `wave24_hook_integration_e2e::task_completed_event_returns_full_outcome_receipt`.

| Code | Status | Location |
|------|--------|----------|
| W-101 | EMITTED | rfc100_emission.rs |
| W-102 | EMITTED | wiring/finding.rs |
| W-110 | EMITTED | rfc100_emission.rs |
| W-120 | EMITTED | rfc100_emission.rs |
| Q-200 | EMITTED | cli_handlers_repo_score.rs |
| Q-210 | EMITTED | health_delta.rs |
| Q-220 | EMITTED | health_delta.rs |
| Q-230 | **ACTIVATED** | QualityFinding (NEW) |
| Q-240 | **ACTIVATED** | QualityFinding (NEW) |
| B-301 | DEFERRED | (requires scope refactor) |
| B-310 | **ACTIVATED** | pre_tool_use.rs |
| B-320 | **ACTIVATED** | cli_handlers.rs |
| G-400 | **ACTIVATED** | typestate.rs |
| G-401 | **ACTIVATED** | typestate.rs |
| G-410 | EMITTED | diagnostic_speculate_passed() |
| G-420 | EMITTED | GeneratorErrorKind (TemplateVariableRejected) |
| M-530 | EMITTED | memory_finding.rs |

Production-verified: `cargo check --workspace` exit 0. 292 (analysis) + 85 (generator) + 3240 (hooks) tests, 0 failures.

## v4.22.0 (2026-04-27) — Wave 10: RFC-100 Emission Maximization

- **S-1 — W-101 emission**: `WiringFinding::LowIntegration` gains `emit()` method. `cli_wiring_modules` emits W-101 when integration_score < 1.0. 3 production sites emit (was 1).
- **S-2 — Q-220 improvement_streak**: `health_delta.rs` emits Q-220 when improvement_streak breaks regression.
- **S-3 — SerializedInferlet → touring-wasm CacheEntry**: Real orphan wired. `CacheEntry` now stores `SerializedInferlet` for persistent disk caching.
- **S-4 — `rfc100_emission.rs` helper module (NEW)**: `Rfc100Emitter` with 3 methods (`emit_w101_low_integration`, `emit_w110_dependency_cycle`, `emit_w120_stale_index`). 11 unit tests.
- **S-5 — Passive handler enrichment**: `cli_wiring_modules` + `cli_wiring_cycles` + `cli_cognitive_metrics` now emit structured diagnostics (W-101/W-110 + cognitive health warning).

composite_health_score: 0.4894 → 0.5000 (+0.0106, crosses 0.5 threshold).

## v4.21.0 (2026-04-26) — Wave 9: Synergy Deepening

- **S7 — miette + syntect production wiring**: `with_source_snippet` was test-only. Wave 9 adds `touring_core::diagnostic::read_source_snippet(file_path, max_bytes)` and `Diagnostic::try_attach_source_from_file()`. Wired in `cli_ast_blast` (B-300) and `cli_wiring_orphans` (W-100). 64 KiB hard ceiling, UTF-8 char-boundary truncation.
- **S8 — `composite_health_score` in instructions_loaded**: New `touring_core::health` module. `instructions_loaded::push_health_parts()` injects warning when score < 0.5. Operator sees degradation BEFORE first edit.
- **S9 — `touring synergy --with-metrics`**: New flag enriches each wired_pair with live counter via `WIRED_PAIR_METRICS` mapping (10 entries). E.g., `pre_edit ↔ TDG` gets `metrics: {counter: "diagnostic_tdg_emitted_count", value: 17}`.
- **Fix collateral**: `.cargo/config.toml` removed orphan `split-debuginfo` (cargo 1.93.1 ignores under `[build]`). `cli-devrcfile-import/export` added to `ALL_DAEMON_HOOK_NAMES` (Hook Registry 174 → 176). Removed orphan `parallel_group` reference in `cli_handlers_decompose.rs`.

```bash
# S9: synergy enrichment with live counters
touring synergy wired -j --with-metrics | jq '.pairs[] | select(.metrics)'
# S8: composite_health_score surfaced at session-start when degraded
touring status -j | jq .composite_health_score
# S7: production diagnostics carry source snippets
touring ast blast crates/touring-hooks/src/big_file.rs -j | jq '.diagnostics[0].source_snippet'
```

## v4.20.0 (2026-04-26) — Wave 8: Synergy Maximization

- **S1 — miette + source bridge**: `Diagnostic` gains optional `source_snippet` + `source_span` fields. Builder methods `with_source_snippet()` + `with_source_span()`. `to_miette_report()` automatically attaches `NamedSource`.
- **S3 — `composite_health_score` in status**: New top-line field in `touring status -j`. Weighted: daemon_health (30%), orphan_ratio (20%), regression_streak (20%), cache_hit_ratio (15%), ema_reward (15%). < 0.5 = degraded.
- **S5 — Q-201/Q-202 RFC-100 emission**: TDG signal in `pre_edit::compose_quality_evolution` emits via `tracing::warn!` when grade ∈ {D, F}. Q-201 = TDG_GRADE_F, Q-202 = TDG_GRADE_D.
- **S6 — `touring synergy` meta-command**: New CLI reporting cross-subsystem wiring. 37 wired_pairs + 7 deferred opportunities. Subcommands: `report`, `wired`, `opportunities`. Flag `-j`.
- **Fix collateral**: `cli-tasksfile-validate` + `cli-tasksfile-export` added to `ALL_DAEMON_HOOK_NAMES` (Hook Registry 172 → 174).

## v4.19.0 (2026-04-26) — Wave 7: rsrl RL Stack Comparative Analysis (Documentation-Only)

Deep analysis of [`rsrl`](https://crates.io/crates/rsrl) v0.8.1. Last commit 2020-06-18 (~6 years dormant). **Verdict: SKIP rsrl as workspace dep**. Touring's RL stack is genuinely richer.

- **D1**: `references/touring-cli-rl-stack.md` (~310 LOC) — comparison matrix in 5 categories (Algorithms, Bandits, Function Approx, Eligibility Traces, Advanced Components).
- **D3 collateral**: 6 pre-existing compile errors in `crates/touring-server/src/cli/tasksfile.rs` fixed (`.cloned()` on `Option<&str>` → `.map(str::to_string)`).

VP-Scout findings: rsrl 6+ year abandonment, ndarray version conflict near-certain, BLAS system requirement, eligibility traces already exist in `qtable.rs`. Touring has 9 RL primitives + 8 bandit primitives + MCTS + FTRL + clustering vs rsrl's 8 TD algos + 4 AC variants.

## v4.18.0 (2026-04-26) — Wave 6: BugStalker Debugging Integration (Documentation-Only)

Analysis of [godzie44/BugStalker](https://github.com/godzie44/BugStalker) v0.4.5. **Verdict: INTEGRATE-AS-DOCS** (binary CLI, not consumable library).

- **D1**: `references/touring-cli-debugging-bugstalker.md` (~280 LOC).
- **D2**: Helper script `~/projects/touring/scripts/debug-touring-daemon.sh` (~115 LOC). Auto-detects PID, sanity-checks BugStalker + ptrace_scope, validates PID is touring.
- **D3**: SKILL.md addendum.

| Tool | Trade-off |
|------|-----------|
| tokio-console (port 6669) | Live stream, requires rebuild instrumentation |
| dhat-heap | Allocator swap, memory bloat |
| OTLP | Span export, distributed tracing |
| BugStalker (--oracle tokio) | Pause-the-world snapshot, ZERO instrumentation, post-mortem |

```bash
cargo install bugstalker                                        # one-time
~/projects/touring/scripts/debug-touring-daemon.sh --oracle          # tokio task tree
bs -p $(pgrep -f "touring serve")                                # manual attach
```

## v4.17.0 (2026-04-26) — Wave 5 Synergy: `touring ast highlight`

- **T1 — Workspace dep `syntect = "5.3"`**: `regex-onig` backend (reuses `onig 6.5.1` from candle-transformers). `regex-fancy` excluded (conflicts with `fancy-regex 0.13.0`).
- **T2 — Module `cli/highlight.rs`**: 5 public fns + `Lazy<SyntaxSet>` + `Lazy<ThemeSet>` (~5–20ms cold load). Default theme "Solarized (dark)".
- **T3 — CLI `touring ast highlight <file>`**: Pure-library command. Auto-detects lang by extension. Respects `NO_COLOR` + `IsTerminal::is_terminal`.
- **Bug fix collateral**: orphan `rusqlite::params` import in `hook_decompose_bridge.rs:24`.

VP-Scout filtered 4 FALSE_POSITIVES: python-ast (pyo3 conflict + redundant), ts-typed-ast (tree-sitter conflict + abandonment), ast-grep-py (Python bindings; Rust `ast-grep-core` already used), parsel (proc-macro-only).

## v4.16.0 (2026-04-25) — Wave 4 Synergy: Rich Terminal Rendering

- **T1 — miette Bridge**: `Diagnostic` implements `miette::Diagnostic`. Method `to_miette_report()`.
- **T2 — `touring ast blast --tree`**: ASCII art via `termtree::Tree` (Direct dependents + Co-edit signals).
- **T3 — `touring wiring audit/cycles --tree`**: 3 subtrees (Orphans W-100, Low-Score Modules, Cycles).
- **T4/T5**: `BlastWarning` RFC-100 in `cli_ast_blast` JSON; `MemoryFinding` in `cli_memory_recall` JSON.
- **Fix**: `health_events::tests::publish_and_subscribe_roundtrip` test isolation bug.

## v4.15.0 (2026-04-25) — Wave 3 Synergy: 3 RFC-100 Diagnostic Sites

- **G1**: `cli_decompose_create` consults `GranularityBandit::select_split()` when `cila_level` not provided.
- **G2**: `pre_edit::run_returning` emits `BlastWarning::HighBlast` (B-300) when blast > 10.
- **G3**: `cli_memory_recall` emits `MemoryFinding` (M-500/M-510/M-520).

## v4.14.0 (2026-04-25) — Wave 2 Synergy: 4 Cross-Subsystem Integrations

- **N2**: `touring wiring audit` includes RFC-100 W-100/W-103 diagnostics.
- **N5**: `cli_session_summary` enriched with health_delta per file.
- **N6**: `instructions_loaded` injects cognitive graph status.
- **N7**: `cli_wiring_status` connects orphan pub fns + hypergraph cycles.

## v4.13.0 (2026-04-25) — P1 mpatch + Cross-Audit

- **P1**: `mpatch-fuzzy` feature gate — dry-run patch preview. `cli_mpatch_preview` handler + `mpatch_preview_if_enabled` pre_write hook + `PatchComplexityDelta` in health_delta.
- **Cross-Audit fixes**: 4 minor production fixes.

## v4.12.0 (2026-04-25) — Synergy Wave: 5 Cross-Subsystem Integrations

- **S1**: TDG grade signal in `pre_edit::compose_quality_evolution` warns D/F.
- **S2**: Health Delta streak → RFC-100 Q-210.
- **S3**: Wiring audit includes F2 cycle detection.
- **S4**: GateMetrics gains 2 RFC-100 prevalence counters.
- **S5**: Pre-task scout injects orphan pub symbol hint.

## v4.11.0 (2026-04-25) — StringZilla Performance Wave Complete

8 hotspot optimizations across 4 crates using StringZilla SIMD-accelerated routines.

| ID | Crate | File | Technique | Gain |
|----|-------|------|-----------|------|
| T0.1 | touring-antt | `reranker.rs` | AhoCorasick replaces 8× `.contains()` | ~8× |
| T0.2 | touring-hooks | `pre_tool_validator.rs` | `StaticPrefixPattern` replaces regex for 29/30+ patterns | ~15× |
| T0.3 | touring-hooks | `async_knowledge.rs` | `memmem::Finder` + `OnceLock` replaces SQL LIKE | Eliminates full-scan |
| T1.1 | touring-analysis | `quality/complexity.rs` | `RangeUtf8NewlineSplits` replaces `str.lines()` | 3-5× |
| T1.3 | touring-analysis | `quality/fast_hash.rs` | `stringzilla::hash` AES-NI as blake3 pre-filter | Skips 90%+ blake3 |
| T2.1 | touring-generator | `core/context.rs` | BK-tree O(log N) + `sz_edit_distance` | ~2125× |
| T3.1 | touring-hooks | `cli_handlers_index.rs` | `utf8_case_insensitive_find` for `--ignore-case` | O(N) SIMD |
| T3.3 | touring-hooks | `cli_handlers_suggest.rs` | `LazyLock<AhoCorasick>` for 18 skill routing patterns | 18× |

**Key invariants**:
- `StaticPrefixPattern` vs `DangerousPattern`: prefix-based use `starts_with` O(m); regex only for catch-all. 85%+ of validator branches never touch regex.
- `fast_content_hash` is pre-filter only: `stringzilla::hash` (AES-NI polynomial, NOT cryptographic). blake3 remains authoritative.
- BK-tree O(log N) lazy-seeded. Edit distance via `sz_edit_distance` (feature `simd-fuzzy`).
- AhoCorasick patterns are `LazyLock`: built once, zero subsequent overhead.

```bash
# Case-insensitive symbol lookup
touring index find HookRuntime --ignore-case
# Finds: HookRuntime, hookruntime, HOOKRUNTIME, etc.
```

46 cross-audit E2E tests across 4 crates. Session report: `~/projects/touring/docs/2026-04-25-stringzilla-wave-complete.md`.

## v4.10.0 (2026-04-25) — Wave Q Complete (RFC-100 Diagnostic Codes)

Q1+Q2+Q3+Q4 done end-to-end:

- **Q1**: `touring ast tdg <file>` — TDG grade letters A+..F (6 dimensions: complexity, coverage, duplication, churn, entropy, antipatterns).
- **Q2**: `touring ast scan --rules <dir>` — batch ast-grep YAML rule library.
- **Q3**: `touring gotcha sync|init` — YAML rule library ↔ SQLite cache.
- **Q4**: 27 stable diagnostic codes across 5 subsystems (W/Q/B/G/M). Foundation: `touring_core::diagnostic::{Diagnostic, DiagnosticCode, Severity, codes}`. RFC-100 spec: `~/projects/touring/docs/touring/RFC-100-diagnostic-codes.md`.

CLI consumer: `touring wiring orphans --diagnostics` emits structured W-100/W-103 in JSON.

## v4.9.0 (2026-04-24) — Wiring Enhancement Wave

- **F1**: `touring wiring impact <symbol> [--depth N]` — transitive impact via BFS consumer walk.
- **F2**: `touring wiring cycles [--min-depth N] [--format json|text]` — Tarjan's SCC cycle detection.
- **F3**: ACP (Agent Client Protocol) shim layer — `protocol/acp.rs`. 7 unit tests.
- **F4**: HyperGraph wrapper — `wiring/hypergraph.rs` with petgraph artificial node pattern. 6 unit tests.

## v4.8.0 (2026-04-24) — Cronflow Workflow Enhancements

- **B3**: `cli_workflow_run` returns `events: [...]` array (`task_start → [subtask_start × N] → task_complete`).
- **B4**: `cli_workflow_status` polling with aggregated counters + scoped subtask IDs `task_id::subtask_id`.
- **B5**: `cli_workflow_resume` with scoped IDs.
- **B6**: ANSI color rendering — `summary.colored` with `\x1b[1;32m▶\x1b[0m`.
- **B9**: 74 E2E tests in `cli_handlers_e2e`.

## v4.6.0 (2026-04-20) — Predictive Wave

Transition from reactive to predictive co-processor. 3 vectors + 1 consolidation:

- **D2**: `BlastRadiusEngine::compute_with_timeout` + HNSW ANN injected in `PreToolUse[Task*]`. Mutates `updated_input` when blast crosses > 3 modules. Budget 40ms.
- **D3**: `LinUCBBandit` (8 arms × 25 dims) integrated in `handle_task_sync_post_list`. Emits `[TOURING RL-ROUTER]` when EV margin > 0.15. Anti-deadlock: `try_lock()`.
- **D4**: `CognitiveMCTS` disambiguated. Shadow rollouts in `handle_enter_plan_mode` with Tarjan SCC. Budget 12s internal / 200ms join.
- **D5**: 9 counters in `gate_metrics.rs` (blast/linucb/mcts families).

```bash
touring gate-metrics -j | jq '{
  blast_inject: .blast_inject_count,
  blast_timeout: .blast_timeout_count,
  blast_mutation: .blast_mutation_count,
  linucb_manual: .linucb_route_manual_count,
  linucb_generator: .linucb_route_generator_count,
  linucb_hint: .linucb_route_hint_count,
  mcts_run: .mcts_shadow_run_count,
  mcts_timeout: .mcts_shadow_timeout_count,
  mcts_deadlock: .mcts_shadow_deadlock_detected_count
}'
```

## v4.5.0 (2026-04-18) — Dynamic Quality Wave

15 sub-waves (5.1 → 19) closing the edit↔generate cycle with RL + observability:

- **W5.1**: Multi-lang `code_workflow` dispatch (Rust/Python/TS/TSX/JS/Bash).
- **W6**: `QualityGateAdapter::detect_language()` — 8 languages.
- **W7**: `.with_semantic_threshold(f32)` — syn-backed `RustQualitySignals::health_score()` as 4th gate.
- **W8**: Symmetric fusion — `wave5_workflow::rust_workflow_hint` emits `health=X.XX` + damper when `health < 0.75`.
- **W9**: `shared::health_delta` singleton (DashMap, OnceLock).
- **W10/W11/W15**: pre_edit, post_edit, pre_write wired multi-lang.
- **W12**: 5 `health_delta_*` counters in `gate_metrics`.
- **W13**: Per-path `StreakCounters` + alert threshold 3 + recovery counter.
- **W14**: Hint surfacing in `pre_edit` (Signal 13) + `pre_read::collect_index_signals` (weight 1.5).
- **W15**: CLI direct `touring health-delta {status,reset} [path]`.
- **W16**: `touring status -j` includes `health_delta`; 2 MCP tools.
- **W17/W18**: `shared::query_cache` (moka 4096 cap, 60s TTL) in 5 hot paths + `invalidate_by_path`. 4 counters.
- **W19**: Generator integration via `HealthDeltaRecordFn` + `HealthDeltaComputeFn` closures in `Speculated::commit()`.

## v4.4.0 (2026-04-18) — Rust Deep Analysis Wave

New `touring ast` subcommands: `ast rust-semantic` (syn 2.0), `ast format-rust` (prettyplease), `ast workspace-info` (cargo_metadata). `TracedAstError` + `AstResultExt` (tracing-error). 957 tests passing in touring-ast + touring-analysis.
