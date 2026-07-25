---
name: touring
description: Master integration skill for the Touring code intelligence stack (CLI + daemon + 22 curated MCP tools (with 102 legacy tools available via `--features mcp-legacy` during 30-day migration)). Use ALWAYS before editing code, creating modules, refactoring, code review, writing tests, or planning architecture in projects under ~/projects/touring/. Invoke when the user mentions Touring, TACO orchestration, file metadata, blast radius, VGP verification, wiring orphans, code generation, MCTS planning, RFC-100 diagnostics, or any of the touring-* subagents (scouter, architect, engineer, auditor, scriber). Provides pre-edit safety gates, symbol verification, RL-backed suggestions, and 120+ CLI commands for code intelligence and quality tracking.
---

# Touring — Master Integration Skill

> **Touring**: v30.3.0 (skill **v5.0.1**, 2026-05-10) | **CLI Commands**: 82 + 27 hooks | **MCP Tools**: 22 curated (+ 102 legacy under `--features mcp-legacy`) | **Hook Registry**: 198 | **Synergy WIRED_PAIRS**: 50 | **Tantivy Schema**: v5 | **Constitution v8.0**: 5 RFCs (001-005) + master doc + 12-audit-suite | **Wave Master Plan**: S1-S8 DONE · S9: D9.1-D9.11 ✅ ALL COMPLETE

For the full version history (v4.4.0 → v5.0.1), see [references/changelog.md](references/changelog.md).

---

## When to Activate

**Always before**: editing code, creating modules, refactoring, code review, writing tests, planning architecture, or any activity involving code under `~/projects/touring/` or related Touring workspaces.

**Skip** for: pure documentation, conversational questions, file reads outside code projects.

---

## Three Mandatory Principles

### 1. File Metadata First (Golden Rule)

Before editing **any** file, run:

```bash
touring ast meta <file> --depth summary -j
```

The output reveals `blast_radius`, `quality_score`, `cognitive_score`, `fan_in/fan_out`. Apply this triage:

| Threshold | Action |
|-----------|--------|
| `blast_radius > 10` | Pause; ask for confirmation OR reduce scope |
| `quality_score < 0.5` | Focus on robustness OR justify the risk |
| Both critical | STOP — plan mitigation first |

For dependency tree, follow with `touring ast blast <file>`. For Rust grade-letter triage, use `touring ast tdg <file>` and STOP at grade D or F (see [references/workflows.md](references/workflows.md) for the full TDG action table).

### 2. VGP — Verified Generation Protocol + Symbol Verification Table

Before generating **any** code, verify symbols exist:

```bash
touring index find <symbol>              # exact lookup
touring generate verify --symbol <name>  # VGP gate
```

If a symbol does not exist: remove it from `symbols_to_verify` in the generation plan. If it does: VGP passes — proceed.

#### Symbol Verification Table (Wave TRM 2026-05-02 — constitutional)

**TODO output JSON de TODA fase TACO** que cita símbolos (function/struct/method/type) DEVE incluir um campo dedicado classificando cada símbolo em categoria canônica COM evidência CLI. Defesa institucional contra alucinação após Wave TRM 2026-05-02 (5 inventões custaram 1 wave de retrabalho).

| Role | Field obrigatório | Categorias canônicas |
|---|---|---|
| **scouter** | `cited_symbols` (per finding) | `found` / `found_via_grep` / `not_found` (Chain 8) |
| **architect** | `symbol_verification` | `verified_existing` / `to_be_created` / `unverified_planned` |
| **engineer** | `symbol_verification` | `imported_existing` / `created_this_subtask` / `modified_existing` (NO `unverified_planned`) |
| **auditor** | `vgp_cross_verification` | re-execute CLI on ≥ 50% upstream sample |
| **scriber** | `documented_symbols` | `verified_existing` / `planned_future` / `deprecated_removed` |

**Anti-padrões automáticos** (composite=0.0, status=failed): `BLOCKED_INVENTED_SYMBOL`, `BLOCKED_UNVERIFIED_LOCATION`, `BLOCKED_PHANTOM_LOCATION`, `BLOCKED_FRAUD_DETECTED`, `BLOCKED_NO_SYMBOL_VERIFICATION`.

Esquema completo (per-role examples, evidence formats, cross-role consequence chain): [references/symbol_verification.md](references/symbol_verification.md). Constitucional cross-cutting: `~/.claude/rules/TACO-subagent.md` (seção CONSTITUTIONAL).

### 3. TACO Phase Level

Classify the task before starting; obey the phase set for that level:

| Level | Phases |
|-------|--------|
| **L0-L1** | Solo mode — orchestrator resolves directly, zero subagents |
| **L2** | Phase 1 (scout) → Phase 5 (engineer) → validate |
| **L3** | Phase 1 → Phase 2 (architect) → Phase 5 → Phase 6 (audit) → validate |
| **L4+** | All phases (0, 1, 2, 3, 4, 4.5, 5, 6, 7) |

**FASE 0 is a GATE** — if `cargo check --workspace` or `touring doctor -j` fails, NO subsequent phase runs. Full protocol in [references/workflows.md](references/workflows.md) and [references/agents.md](references/agents.md).

### 4. Elite 50-Dimension Quality Gate (Premium de Elite de Mercado)

Toda entrega TACO/Touring deve atingir o tier-alvo nas **50 dimensões** de elite (F1.1–F4.12), medidas pelo motor real `touring-quality`. Floor mínimo de entrega = **Gold (0.80)**; release = **Diamond (0.95)**.

```bash
touring-quality score <FILE> --dims F1.1,F2.5 --format json    # granular por dimensão
touring-quality check --gate F2.1 --target <FILE>              # 1 dim (P0 < 0.5 = ⛔ BLOCK)
touring-quality score <DIR> --workspace --fail-below 0.80      # gate de entrega (exit 1 se abaixo)
touring-quality list                                           # 50 dims + glyph (⛔ BLOCK / ⚠ WARN)
python3 ~/projects/touring/docs/elite_aggregate.py --check         # release composite (13 gates → touring-elite)
```

**⚠ NÃO existe** `touring quality` (subcommand), `score --gate`, `--enforce`, nem `generator de qualidade dedicado (inexistente)` (PLANNED W7). Remediação real = `Edit tool` + re-score.

**6 BLOCK dims (P0, fail-closed pré-Write)**: F2.1 OWASP · F2.4 secrets · F2.5 dep CVEs · F2.6 config · F4.3 deprecated · F4.5 pkg-mgmt — rode `touring-quality check --gate <dim> --target <FILE>` antes de Write/Edit nessas dims.

**dim → agent owner**: scouter (F1.7-1.8) · architect (F1.9-1.12, F2.13, F3.10, F4.8-4.10) · engineer (F1.1-1.6, F2.1-2.4, F2.7-2.12, F4.1-4.4, F4.6) · auditor (F2.5-2.6, F3.1-3.7, F4.5, F4.12) · scriber (F3.8-3.13, F4.7, F4.11). Catálogo + per-dim reference: `~/.claude/rules/elite-50-quality.md` (keystone) + `~/.claude/skills/touring-elite/references/quality/D01..D52.md` + índice `quality/README.md`. Reflexos 10-12 (Dim-Score-Verify / Dim-Enforce-Block / Dim-Auto-Remediate) no keystone.

---

## Bundled Scripts (Layer 3) — Premium Shortcuts

The skill ships **11 composition shortcuts** + a **5-script Diagnostic Arsenal**
(below) in `scripts/` that compose multiple CLI calls into single high-leverage
operations. Zero-LLM, fail-open
when daemon degraded, dual-output (`--json` machine | human-readable default).
**Prefer the script over re-deriving the same 5-call sequence by hand** —
this is the Layer 3 leverage the quality rubric Gate 4 demands.

| Script | Replaces | Use case |
|---|---|---|
| `scripts/read_file.py <file>` | `ast meta` + `ast blast` + `ast tdg` + `ast rust-semantic` + `ast overview` | **C02** Reading-Comprehend in one shot, with triage verdict |
| `scripts/pre_edit_gate.py <file>` | `ast meta` + `ast blast` + `pre-edit` + `ast tdg` + `gotcha match` + `memory recall` | **C04 + Reflex #1** GO/CAUTION/NO_GO before any Edit |
| `scripts/vgp_batch.py <syms...>` | `index find` + `ast find` + `generate verify` × N | **VGP** for the constitutional Symbol Verification Table |
| `scripts/discover_workspace.py [root]` | `ast workspace-info` + per-crate LOC/symbol/feature sweep | **C10** workspace structure map |
| `scripts/discover_symbol.py <sym>` | `index find` + `ast find` + `wiring impact` + polyglot grep | **C03 + Cadeia 4/4b** symbol forensics + homonimia detection |
| `scripts/diagnose_health.py` | `doctor` + `status` + `gate-metrics` + `learning status` + `evolution drift` | **C12 + TACO FASE 0** traffic-light health gate (exit 0/1/2) |
| `scripts/diagnose_wiring.py` | `wiring orphans` + `wiring chains` + `wiring audit` + Cadeia 7 grep | Real-orphan vs stale-wiring classification per symbol |
| `scripts/analyze_blast.py <files...>` | `ast blast` + `wiring impact` + `ast blast-cross-feature` + `wiring cycles` | **C06 + C11** multi-file blast risk (LOW/MEDIUM/HIGH/CRITICAL) |
| `scripts/analyze_quality.py [path]` | per-file `ast meta` + `ast tdg` + `health-delta status` sweep | **C09** hot-spot ranking + regression detection |
| `scripts/analyze_callers.py <fn_a> <fn_b>` | `ast find` × N + body extraction + call-set diff | **Cadeia C08** cross-caller asymmetry matrix (anti-bug) |
| `scripts/lib_touring.py` | — | Shared CLI wrapper + JSON helpers (imported by the other 10) |

Every script accepts `--help`, `--json`, `--quiet`, `--timeout`. Run any with
`--help` for full options. The scripts are the canonical entry point for the
Touring Decision Matrix categories (C02-C12) — when prose in this SKILL or in
`rules/touring-decision-matrix.md` says "run X + Y + Z", the script is the
already-composed version.

**Diagnostic Arsenal (Layer 3 — 5 systemic diagnostics, `scripts/`; all `main()`-guarded, importable, unit/chain/e2e-tested ≥90% branch)** — shared by loop-engineering + TACO-cross-audit + the subagents; artifacts land in `$DIAG_OUT` or the cwd:

| Script | Fuses / measures | Modus-operandi role → consumers |
|---|---|---|
| `scripts/systemic_diag_v2.py [path]` | 50-dim × architecture(blast) × security(6 P0 + cargo-audit CVE), fused per crate/dir/file | **integrated risk** ranked by enforcement×blast → auditor, cross-audit HARMONY, loop convergence |
| `scripts/crate_50dim_matrix.py <crate>` | complete **lossless** 50-dim (raw JSON + wide + long TSV) at file/dir/crate | full per-dim evidence → auditor, loop-diagnose |
| `scripts/workspace_arch_diag.py [root]` | inter-crate DAG: Tarjan SCC cycles, layers, fan-in(=blast), God-crates | workspace architecture map → architect, scouter, cross-audit MAP |
| `scripts/crate_arch_diag.py <crate...>` | intra-crate God-objects + module fan-in + F1.7/1.8/1.11/1.12 | cohesion/coupling per crate → architect, scouter |
| `scripts/clone_blocks.py <file...>` | Type-1 6-line clones (real-dedup vs scaffold-FP) | classify F1_3 **before** dedup → engineer, cross-audit DEBT |

Tests: `scripts/test_<name>.py` (`python3 -m unittest`, cargo/touring-quality mocked). Never write a matrix into the skill dir — `DIAG_OUT` / cwd only.

**⛔ Reporting Contract (MANDATORY — `scripts/report_contract.py`)**: after running ANY arsenal diagnostic, the result MUST be relayed to the user as a **premium-elite audit report** — every section below, in full depth. Each script prints the contract as its digest footer, so the obligation travels *in the tool output* and survives compaction (the failure mode that once narrowed a whole-workspace report to a one-lever summary, 2026-07-03). Seven sections, in order:

1. **VERDICT** — executive headline: tier/severity + the one thing that matters (≤3 lines)
2. **SCORECARD** — 6 P0 BLOCK gate status · tier distribution · composite
3. **FINDINGS** — BLOCK → WARN → ADVISORY, **EVERY** dim/unit with a finding (full breadth, not the top few)
4. **FUSED RISK** — ranked units: weighted quality defect-load × architecture blast
5. **ROOT-CAUSE** — the counterfactual lever(s) that unlock the tree
6. **PROVENANCE** — enforcement read from source (verify, don't assert) + lossless artifact path
7. **ACTIONS** — prioritized remediation (REGRA #0); flag the human-decision items

A single-lever or top-N summary that **replaces** the full breakdown is a contract violation — the lever is synthesis layered *on top of* the complete matrix, never a substitute. Enforced by `test_report_contract.py` + a contract-presence test in each of the 5 script suites.

**Native master commands (R3/R5/F1/F0 — code-mode without MCP)**: `touring scout/read/health/guard/map/blast/investigate/explore/adw` are CLI wrappers (`touring-server cli/master.rs`) that forward to these scripts — one memorable command instead of a script path, dispatched through the same `touring` binary (no MCP). `explore` (F1) is the loop-until-dry multi-lens exploration with the CCE convergence contract; `adw` (F0) is the durable declarative agent-workflow runner (spec TOML, fsync'd journal + `--resume-run` replay, Class-D narrative-vs-verdict detection, `lint`/`test`/`from-template`). The R6 gate `scripts/harness_gate.py` holds the whole surface to 50-dim Gold (≥0.80). Opt-in SessionStart topic map via `TOURING_INVESTIGATE_ON_START=1`.

**ADW — Software Factory (F0-F6, plan 2026-07-19)**: declarative durable agent workflows. Specs live in `.touring/adw/<name>.toml` (typed nodes `code`/`agent`/`gate`/`loop`/`human`; edges `on_pass`/`on_fail`/`on_dry`); the runner (`touring adw run`) owns loop termination (Law L2), decides node success by gates + Class-D narrative-vs-verdict detection (Law L3), and persists a fsync'd journal with `--resume-run` replay (kill -9 safe). Library templates: `touring adw from-template bugfix|chore|feature|hotfix|audit|explore-plan|scout-perpetuo` (central `adw-library/` + `tiers.toml`). Router: `touring factory route|start "<ticket>"` — deterministic-first, RL-fed. Perpetual scout: `scout_perpetuo.py cycle|status` (yield-adaptive cadence, tickets, act-vs-wait gate). ZTE: human nodes with `zte = true` bypass via conformal `calibrate-confidence` + warm-up, audited in the journal. Racing: `touring adw race <name> --lanes N` (first-to-pass wins, losers canceled, winner-only merge). KPIs: `touring.adw.*` in `touring kpi -j`. Restriction (proven A/B): headless agents cannot write under `~/.claude/` — point agent-editing ADWs at projects outside it.

**The 4 Pillars — first reflex** (task #6 compounding; full rule `~/.claude/rules/touring-4-pillars.md`): for code work, reach for the differential *before* the atomic/raw tool — **Code Mode** (`touring run`, no MCP) over shell loops/scans · **Master CLI** (`scout/read/map/blast/investigate/guard/audit`) over chained `index find` + `ast blast` + `wiring` atomics · **Learning Memory** (`touring memory recall "<topic>"`) before researching from scratch · **Intelligence** (`touring ast/index/wiring`) over guessing structure. The active hook layer (`cli_suggester` pillar induction, **default-OFF** via `TOURING_PILLAR_INDUCTION_ARMED`) nudges the two under-used (Master CLI, Learning Memory); adoption is measured via `touring.coupling.pillar_induction_ratio` (`touring kpi -j`). **Injection-density invariant**: every nudge and every answer is dense, specific (real argument, no `<placeholder>` when derivable), and grounded in a named best-practice.

---

## CLI Command Ranks (decision-time guide)

### TIER 1 — Critical (always use, directly affect quality)

| ★★★★★ | Command | Why | When |
|--------|---------|-----|------|
| `touring ast meta <file> --depth summary -j` | File metadata first | blast_radius, quality, cognitive, fan_in/fan_out | Before any Edit |
| `touring pre-edit` | Pre-edit hook | Composite score 0–1 with CILA budget + rayon signals | Before each Edit; require ≥ 0.8 |
| `touring ast blast <file>` | Blast radius | Full dependency tree | Before refactors L3+ |
| `touring index find <symbol>` | Symbol lookup (VGP) | Verify symbol exists | Before generating code |
| `touring wiring orphans -j` | Orphan detection | Pub symbols without consumers | After creating new pub fn/mod |
| `touring e2e -j` | E2E health | Composite system score 0–1 | Before risky changes |

### TIER 2 — Diagnostics (system health)

| ★★★★☆ | Command | Output |
|-------|---------|--------|
| `touring doctor -j` | daemon_socket, daemon_health, circuit_breaker, project_db |
| `touring status -j` | symbol_count, orphan_count, ema_reward, **composite_health_score**, health_delta |
| `touring synergy [report\|wired\|opportunities] [-j] [--with-metrics]` | Cross-subsystem wiring observability (50 wired_pairs after Wave TRM 2026-05-02, 16 metrics-enriched via WIRED_PAIR_METRICS) |
| `touring gate-metrics -j` | All counters (rkyv, tantivy, health_delta, query_cache) |
| `touring learning status` | LinUCB arms, EMA reward, converging state |
| `touring health-delta status [path]` | Per-path streak + warning hints |

### TIER 3 — Intelligence (deep analysis)

| ★★★★☆ | Command | Output |
|--------|---------|--------|
| `touring wiring impact <symbol> [--depth N]` | Transitive impact (BFS) |
| `touring wiring cycles [--min-depth N]` | Tarjan SCC cycle detection |
| `touring ast blast-cross-feature <file>` | Cross-feature dependency analysis |
| `touring ast rust-semantic <file.rs>` | syn — generics, trait bounds, lifetimes, derives, semantic_complexity |
| `touring ast format-rust <file.rs> [--preserve]` | rustfmt-clean output (prettyplease, --preserve keeps doc positions) |
| `touring ast workspace-info [<dir>]` | cargo_metadata: packages, features, dependents_of |
| `touring ast grep <file> <pattern> [--rewrite <r>]` | Polyglot structural search + rewrite (ast-grep) |
| `touring ast highlight <file>` | syntect ANSI rendering (NO_COLOR honored) |
| `touring ast tdg <file>` | TDG grade letter A+..F (6 dimensions) |
| `touring wiring audit -j` | Full orphans + low-score modules |
| `touring file-knowledge extended <file>` | 23 metadata fields |
| `touring tantivy search "<query>"` | BM25 ranked search |
| `touring assist list-kinds \| applicable \| apply <kind> <file>:<line>` | 10 assist handlers (auto_wire, extract_function, inline_call, etc.) |
| `touring ssr {status \| apply --pattern <pat> --replacement <repl> [--lang <l>] [--stdin]}` | Semantic structural rewrite (pattern==>>replacement). **Note**: `apply` reads only from `--stdin`; for in-place file rewrite use `touring ast grep <file> <pat> --rewrite <repl>` |
| `touring skip list \| validate <file>` | SkipContext region markers (W-115) |
| `touring source-change apply [--path <f>]` | SourceChange transactional apply via Applier |

### TIER 4 — Session / Checkpoint

| ★★★★☆ | Command | Use |
|--------|---------|-----|
| `touring session start [id] type "<obj>"` | Init session + load knowledge + RL state |
| `touring session assess [id]` | Composite score + phase breakdown |
| `touring decompose create <type> "<desc>" [--origin=X --cila-level=N]` | Create DAG |
| `touring decompose add <task> <subtask> [deps]` | Add subtask (deps comma-separated) |
| `touring memory store <key> <val> --tier semantic` | Persist lesson |
| `touring profile query <file> \| dump [--output <f>] \| heap-dump \| flamegraph` | Hotpath RAII instrumentation (touring-core::profile) |

### TIER 5 — Code Generation (touring-generator)

| ★★★★☆ | Command | Pipeline stage |
|--------|---------|----------------|
| `touring generate list-kinds -j` | Discovery (36 kinds — drift fix + Wave TRM crate-scaffolding kinds) |
| `touring generate verify --symbol <name>` | VGP verification |
| `touring generate render <kind> [--vars '{}']` | Template render preview |
| `touring generate plan-speculate --file <path>` | Shadow validate |
| `touring generate plan-submit --file <path>` | Atomic commit (Draft→Verified→Rendered→Speculated→Committed) |

### TIER 6 — Learning / RL

| ★★★★☆ | Command | Effect |
|--------|---------|--------|
| `touring learning reward <tool> <val> [ctx]` | Inject reward → updates LinUCB + QTable |
| `touring evolution drift -j` | Alert level: none\|degraded\|structural |
| `touring evolution insights -j` | Tool effectiveness stats |

### TIER 7 — Hooks (Claude Code lifecycle)

| ★★★☆☆ | Command | Hook |
|--------|---------|------|
| `touring serve` | Daemon startup (idle watchdog OPT-IN via `TOURING_IDLE_TIMEOUT_SECS>0`) |
| `touring pre-read` / `post-read` | Read enrichment + co-edit graph update |
| `touring pre-write` / `post-edit` | Speculative validation + quality tracking |
| `touring pre-grep` / `pre-glob` | Symbol enrichment for Grep/Glob (D43, P99=2ms; disable: `TOURING_DISABLE_PREGREP=1`) |
| `touring instructions-loaded` | Session-start context injection |
| `touring cortex <event>` | Unified fascicles dispatcher |

### TIER 8 — Search / Index (read-only, <10ms)

| ★★★☆☆ | Command | Use |
|--------|---------|-----|
| `touring index status` | Index health |
| `touring index search <prefix>` | Prefix lookup |
| `touring tantivy fuzzy "<query>" [dist]` | Levenshtein fuzzy |
| `touring tantivy suggest "<prefix>"` | Autocomplete |
| `touring search symbols "<query>"` | BM25 rank |

### TIER 9 — Utility

| ★★☆☆☆ | Command | Use |
|--------|---------|-----|
| `touring gotcha list/match` | Pitfall DB |
| `touring memory recall "<query>"` | FTS5 + cosine |
| `touring diary write/read/projects <agent>` | Agent diary (AAAK) |
| `touring decompose finalize/ready` | Archive task / list ready subtasks |
| `touring inferlets list/run` | WASM sandbox inference (L7-B) |
| `touring jobs spawn/poll/list` | Background workers (L7-B) |
| `touring health-delta reset <path>` | Clear streak post-refactor |

---

## Quick Cheatsheet

```bash
# PRE-EDIT (mandatory order)
touring ast meta <file> --depth summary -j   # 1. file metadata first
touring ast blast <file>                     # 2. blast radius
touring pre-edit                             # 3. score >= 0.8
touring index find <symbol>                  # 4. VGP

# DIAGNOSTICS
touring doctor -j                            # health check
touring status -j                            # dashboard + composite_health_score
touring synergy --with-metrics -j            # 50 wired_pairs + 16 live counters (Wave TRM 2026-05-02)
touring gate-metrics -j                      # full counter set

# WIRING
touring wiring audit -j                      # full audit
touring wiring orphans -j                    # orphans
touring wiring impact <symbol> --depth 2     # transitive impact

# SESSION + DECOMPOSE
touring session start <id> type "<obj>"
touring decompose create <type> "<desc>" --origin=touring-cli --cila-level=3
touring decompose add <task> <sub> [deps]

# MEMORY + LEARNING
touring memory store <key> <val> --tier semantic
touring memory recall "<query>"
touring learning reward <tool> <val> [ctx]

# GENERATE
touring generate list-kinds -j
touring generate verify --symbol <name>
touring generate plan-submit --file <plan>

# TANTIVY SEARCH
touring tantivy search "<query>"
touring tantivy fuzzy "<query>" 2

# COMPUTE EXECUTE (Think-in-Code — S2, Reflex #8)
touring_ctx_execute language="python" code="import json; print(len(sys.argv))" args='["a","b"]'  # ctx_execute MCP tool (200× compression)
# Supports: js/python/ts/ruby/go/rust/shell/perl/php/elixir — wraps SandboxExecutor, forbidden_calls detection

# SYMBOL VERIFICATION (Wave TRM 2026-05-02 — constitutional, all roles)
touring index find <SymbolName> -j        # primary verification
touring ast find <SymbolName> -j          # signature + module path
touring ast overview <file> -j            # post-edit confirm (engineer Phase 4.5)
touring decompose status -j               # confirm to_be_created subtask exists (auditor Phase 0.6)

# RUST DEEP (Wave 4)
touring ast rust-semantic <file.rs>          # syn — generics, traits, semantic_complexity
touring ast format-rust <file.rs>            # prettyplease (no rustfmt binary)
touring ast workspace-info                   # cargo_metadata
```

---

## Subagent Pool (TACO)

Six specialized agents. Invoke via `Agent` tool. All return raw JSON. **Subagents inherit the orchestrator's permission mode — spawn with the SAME permissions (omit `mode` in the `Agent` call); NEVER force a narrower `acceptEdits`, which makes the subagent prompt for every Bash command when the session is on `auto`/`bypassPermissions`.** Ensure the orchestrator is on `acceptEdits`+ before spawning engineers so edits are enabled by inheritance.

| Agent | When |
|-------|------|
| **touring-scouter** | Scouting, blast, VP-Scout, orphans |
| **touring-architect** | Architecture, MCTS, Context7 |
| **touring-engineer** | Implementation, refactor, VGP-verified codegen |
| **touring-auditor** | Cross-audit, E2E creation, scope max |
| **touring-scriber** | Documentation, changelogs, ADRs |

Full delegation rules + prompt templates in [references/agents.md](references/agents.md).

---

## Best Practices by Category

Detailed workflow guides in [references/workflows.md](references/workflows.md):

- **PRE-EDIT** — file metadata + blast + pre-edit score + VGP
- **INTELLIGENCE** — blast/wiring/cognitive/file-knowledge analysis
- **LEARNING** — reward injection, drift, insights
- **MEMORY** — semantic store, recall, diary
- **GENERATE** — VGP-verified pipeline (5-stage typestate)
- **DECOMPOSE** — task DAG lifecycle (Pln2/Pln3 flags)
- **TACO Phase Protocol** — FASE 0 health gate, FASE 4.5 anti-FP gate

---

## Golden Rules

1. **File metadata first** — `touring ast meta` before any Edit
2. **Always** run `touring doctor -j` before critical phases
3. **Always** use `touring index find` before creating new symbols
4. **Always** use `touring shadow validate` (or `plan-speculate`) before Edit/Write
5. **Always** run `touring wiring audit` after creating new pub modules
6. **Never** ignore orphan symbols — they indicate unwired code
7. **Always** persist lessons via `touring memory store`
8. **Use** `touring evolution` to update patterns after errors
9. **VGP** — verify via `touring generate verify --symbol <name>` before generation
10. **Subagents inherit the orchestrator's permission mode** — spawn with the SAME permissions (omit `mode`); NEVER force a narrower `acceptEdits` (it prompts for Bash on every command under `auto`/`bypassPermissions`). Be on `acceptEdits`+ yourself so engineers can edit by inheritance.
11. **Symbol Verification Table MANDATORY** (Wave TRM 2026-05-02) — every JSON output citing a symbol DEVE include the role-specific verification field (`cited_symbols` / `symbol_verification` / `vgp_cross_verification` / `documented_symbols`) with CLI evidence. Cite without `touring index find` output = `BLOCKED_INVENTED_SYMBOL` = composite 0.0. Schema: [references/symbol_verification.md](references/symbol_verification.md).

---

## Context Window Selection

| Channel | Latency | Use for |
|---------|---------|---------|
| CLI (`touring`) | <10ms | read-only queries (index, wiring, memory recall) |
| MCP (`mcp__touring__*`) | ~200ms | write operations (store, decompose, suggest) |
| Bash (speculate) | <200ms | speculative validation |

**Rule**: prefer CLI for read-only queries. MCP for writes and complex analysis.

For the token-efficient MCP workflow (`touring_minimal_context` → `detail_level='minimal'` → follow `_next_tools`), see [references/api_reference.md](references/api_reference.md).

---

## Reference Map

Operational depth (consult on demand):

| Topic | File |
|-------|------|
| Workflows by category + TACO phases + TDG action table | [references/workflows.md](references/workflows.md) |
| 6 subagent pool, delegation, prompt templates | [references/agents.md](references/agents.md) |
| **Symbol Verification Table** (Wave TRM 2026-05-02 constitutional) — schema per role + anti-padrões + cross-role consequence chain | [references/symbol_verification.md](references/symbol_verification.md) |
| Public Rust APIs + MCP catalog + token-efficient workflow | [references/api_reference.md](references/api_reference.md) |
| Wave history v4.4.0 → v4.23.0 (changelog) | [references/changelog.md](references/changelog.md) |
| StringZilla, GPU, ACP, rkyv, supply-chain, Rust deep, dynamic quality | [references/integrations.md](references/integrations.md) |
| TACO phase protocol detail | [references/taco_protocol.md](references/taco_protocol.md) |
| 3-layer CLI architecture, daemon actor, dispatch table | [references/architecture.md](references/architecture.md) |
| Code generator (31 kinds, typestate pipeline) | [references/code_generator.md](references/code_generator.md) |
| **Touring-native tooling** (deterministic codegen wrapper for Rust/Python/TS — consumes touring-generator) | `~/.claude/skills/Touring-native tooling/SKILL.md` + `~/.claude/skills/Touring-native tooling/references/touring-integration.md` |
| MCP tools catalog (22 curated, 102 legacy) | [references/mcp_tools.md](references/mcp_tools.md) |
| **Code Mode recipes** (Reflex #8 cookbook — 5 patterns for `touring_ctx_execute` programmatic tool-calling, 80-96% token savings) | [references/code_mode_recipes.md](references/code_mode_recipes.md) |
| Touring CLI by cluster (7 modules) | [references/touring-cli-overview.md](references/touring-cli-overview.md), [hooks](references/touring-cli-hooks.md), [intelligence](references/touring-cli-intelligence.md), [tasks](references/touring-cli-tasks.md), [rl-quality](references/touring-cli-rl-quality.md), [generate](references/touring-cli-generate.md), [meta](references/touring-cli-meta.md), [assists](references/touring-cli-assists.md) |
| RL stack comparison (Touring vs rsrl) | [references/touring-cli-rl-stack.md](references/touring-cli-rl-stack.md) |
| BugStalker debugging integration | [references/touring-cli-debugging-bugstalker.md](references/touring-cli-debugging-bugstalker.md) |
| Auto-loaded CLI ranks (constitutional) | `~/.claude/rules/touring-cli-index.md` |
| **Constitution v8.0** (S9 — H3.3) | `~/projects/touring/docs/CONSTITUTION-v8.md` (master, 416L) |
| RFC index (001-005) | `~/projects/touring/docs/RFC-001*.md` · `RFC-002*.md` · `RFC-003*.md` · `RFC-004*.md` · `RFC-005*.md` |
| Constitution v8 audit suite | `~/.claude/audits/2026-05-09-constitution-v8-audit/` (12 scripts, 303 assertions) |
