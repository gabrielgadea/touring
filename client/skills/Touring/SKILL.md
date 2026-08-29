---
name: touring
description: Master integration skill for the Touring code intelligence stack (CLI + daemon + 23 MCP tools in the `tools/list` handshake; ~119 more are catalogued and invocable by name via `tools/call`). Use ALWAYS before editing code, creating modules, refactoring, code review, writing tests, or planning architecture in projects under ~/projects/touring/. Invoke when the user mentions Touring, TACO orchestration, file metadata, blast radius, VGP verification, wiring orphans, code generation, MCTS planning, RFC-100 diagnostics, or any of the touring-* subagents (scouter, architect, engineer, auditor, scriber). Provides pre-edit safety gates, symbol verification, RL-backed suggestions, and 120+ CLI commands for code intelligence and quality tracking.
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

**Os cinco princípios operacionais desta família** (compor-não-copiar · convergência medida · gauntlet cego · grafo plano · Wayfinder) têm um enunciado canônico único, com o comando real de cada um: `references/skill-operating-principles.md`. Uma skill DERIVA a própria instância de lá e liga de volta — jamais repete um banner (`rules/touring-4-pillars.md`: *"A generic banner does not induce — it is debt"*). Os masters abaixo são o P1 aplicado à descoberta: um comando funde as N buscas atômicas que a pergunta exigiria.

**Native master commands (R3/R5/F1/F0 — code-mode without MCP)**: `touring scout/read/health/guard/map/blast/investigate/explore/adw` are CLI wrappers (`touring-server cli/master.rs`) that forward to these scripts — one memorable command instead of a script path, dispatched through the same `touring` binary (no MCP). `explore` (F1) is the loop-until-dry multi-lens exploration with the CCE convergence contract; `adw` (F0) is the durable declarative agent-workflow runner (spec TOML, fsync'd journal + `--resume-run` replay, Class-D narrative-vs-verdict detection, `lint`/`test`/`from-template`). The R6 gate `scripts/harness_gate.py` holds the whole surface to 50-dim Gold (≥0.80). Opt-in SessionStart topic map via `TOURING_INVESTIGATE_ON_START=1`.

**ADW — Software Factory (F0-F6, plan 2026-07-19)**: declarative durable agent workflows. Specs live in `.touring/adw/<name>.toml` (typed nodes `code`/`agent`/`gate`/`loop`/`human`/`parallel`; edges `on_pass`/`on_fail`/`on_dry`/`on_escalate`); the runner (`touring adw run`) owns loop termination (Law L2), decides node success by gates + Class-D narrative-vs-verdict detection (Law L3), and persists a fsync'd journal with `--resume-run` replay (kill -9 safe). Library templates: `touring adw from-template bugfix|chore|feature|hotfix|audit|explore-plan|scout-perpetuo` (central `adw-library/` + `tiers.toml`). Router: `touring factory route|start "<ticket>"` — deterministic-first, RL-fed. Perpetual scout: `scout_perpetuo.py cycle|status` (yield-adaptive cadence, tickets, act-vs-wait gate). ZTE: human nodes with `zte = true` bypass via conformal `calibrate-confidence` + warm-up, audited in the journal. Racing: `touring adw race <name> --lanes N` (first-to-pass wins, losers canceled, winner-only merge). KPIs: `touring.adw.*` in `touring kpi -j`. Restriction (proven A/B): headless agents cannot write under `~/.claude/` — point agent-editing ADWs at projects outside it.

**ADW — flow portfolio (plan 2026-08-18)**: flows are now composed, not copied. `[[use]] module/as/with` inlines a **fragment** under a namespace (`recall.memory`) — resolution happens in the loader, so the engine, journal, resume and lint keep operating on a flat spec. Fragments declare `[fragment] inputs/entry` and leave through the seams `__exit__`/`__exit_fail__`, which the host wires; the kit ships `recall-pack · prior-art · diagnose-pack · fanout-lenses · gate-rust · gate-quality50 · conflict-guard · human-approve · phase-close · converge · critic-panel`. Every shipped flow declares `[purpose]` (`intent · when_to_use · when_not_to_use · inputs · produces · tags`) — `when_not_to_use` is what lets the portfolio rule a flow OUT instead of pitching the closest candidate, and the miner indexes that block BEFORE the header comment so the 600-char cap falls on boilerplate. New surface: `touring adw fragments` (what is composable), `touring adw explain <name>` (the FLAT resolved graph — composition is never the only representation), `touring adw new <name> --intent … --verdict reuse|extend|supersede|create_new --use <frag>[:as] --job <name>[:type] --bind as.input=… --when-not-to-use …` (prior art with an explicit verdict is mandatory; the flow is born lint-clean with its gate feedback already wired).

**Per-node persona (B3)**: `[node.X.persona]` declares posture INLINE — `role · stance · lens · scope · bar · burden · refuses[] · forbids[] · blind_to[] · escalate_when · emits` — compiled to `--agents '{…}' --agent <role>`. No global agent catalogue, so a flow stays portable. A persona with `stance = "reject_by_default"` is a critic and the lint holds it to a critic's structure: a parseable `emits`, never `session = "resume_on_fail"`, and declared blindness that the prompt actually honours.

**Read-only fan-out (B4)**: `type = "parallel"` with a **mandatory** `merge` (`collect|tally|concat` — no default, because N branches sharing one result slot is last-write-wins), `on_branch_fail` (`all` fail-closed | `any` | `ignore`/`best_effort` | `quorum:N`) and `max_branches`. Branches come in two shapes: **static** `branches = ["a","b"]`, or **dynamic** `branches = "{{vars.lenses}}"` + `template = "<node>"` — the template is cloned once per runtime value, bound to `{{branch.value}}`/`{{branch.index}}` (LangGraph's `Send`; the only expression of the canonical orchestrator-workers pattern). `max_branches` is checked BEFORE any branch runs, so exceeding it refuses the block instead of truncating the work. A dynamic template whose persona has a FIXED `lens` is rejected — N clones of one lens buy one opinion N times. Every branch journals as an ordinary node, so `kill -9` mid-fan-out resumes without re-running the ones that finished; `tally` re-emits an aggregate `NEW_FINDINGS=` so a loop can wrap a whole fan-out. Branches that can write are rejected by the lint. `touring adw test` walks a graph that never ran: an agent with no recording is stood in for by a stub **named in the report** (`synthesized`), and a human gate is auto-approved — a spec that DECLARES `driver = "mock"` still demands its recording, so synthesis never leaks into a real run.

**Verification contract (B6)**: a gate with `verdict_contract = true` speaks `VERDICT=PASS|REJECT|ESCALATE` and must declare `on_escalate`. An unparseable verdict reads as REJECT (silence never clears a gate). ESCALATE routes aside **without spending a retry** — a check that could not run is not something more agent attempts will fix. Plus opt-in `stagnation_rounds = N` (identical consecutive rejections stop the loop), a per-run kill switch (a `STOP` file in the run dir), and `[adw] budget_usd` checked against measured spend at node boundaries.

**Wayfinder over the DAG (C3)**: `touring decompose ticket <task> <subtask> --kind decision|implementation --subtype research|prototype|grilling|task --autonomy hitl|afk --fog clear|hazy|unknown --origin-ticket <id>` and `touring decompose frontier <task>` — open decisions **gate** the implementation frontier, and implementation carrying no `origin_ticket` is reported as untraceable (map-as-index: the reasoning lives in the ticket, the map keeps a pointer). Only `research` decisions may run `afk`. **Atomic claim (C2)**: `touring decompose claim <task> --owner <id> [--lease-secs N]` / `release` — `ready` only READS, so two sessions polling it receive the same subtask; claiming is a conditional UPDATE, so exactly one wins.

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

> Export SCIP: a API Rust (`ScipEmitter`/`ScipDocument`) existe e é usada; o
> comando `touring scip emit` NÃO existe. Detalhe em
> `references/scip-export.md` (skill `touring-scip` removida 20/08/2026).

> Comandos de `generate` e de evolução/flywheel que só as skills removidas
> `touring-generator` / `touring-evolve` documentavam foram preservados em
> `references/generate-and-evolution.md` (20/08/2026). As skills saíram por
> colisão de ativação + comandos inexistentes; os 18 comandos reais, não.


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
| `touring-hook pre-grep` / `pre-glob` | Symbol enrichment for Grep/Glob (D43, P99=2ms; disable: `TOURING_DISABLE_PREGREP=1`) |
| `touring-hook instructions-loaded` | Session-start context injection |
| `touring cortex <event>` | Unified fascicles dispatcher |

### TIER 8 — Search / Index (read-only, <10ms)

| ★★★☆☆ | Command | Use |
|--------|---------|-----|
| `touring index status` | Index health |
| `touring index search <prefix>` | Prefix lookup |
| `touring tantivy fuzzy "<query>" [dist]` | Levenshtein fuzzy |
| `touring tantivy suggest "<prefix>"` | Autocomplete |
| `touring search unified "<query>"` | Busca multi-backend fundida por RRF (o default) |
| `touring search exact "<query>"` | Símbolo exato (daemon `cli-search-symbols`) |
| `touring search bm25 "<query>"` | Documentos por BM25 (daemon `cli-search-docs`); `fuzzy` p/ aproximado |
| `touring search tools "<intenção>"` | Achar o comando pelo que você QUER fazer |
| `touring portfolio "<intento>"` | **Prior-art por PROPÓSITO** (não por nome) — 3 seções: candidatos com evidência, lacunas, lente externa; exige veredito `reuse\|extend\|supersede\|create_new`. Subcomandos: `refresh` (minera ~4k artefatos em ~190ms) · `status` · `verdict`. Bilíngue pt/en. Consultar **antes de criar** qualquer artefato novo (REGRA #0) |

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


## Context Enrichment — the doctrine (2026-08-08)

Everything that puts information in front of the agent unasked — hook
`additionalContext`, an ADW node summary, a CCE lens, a prior-art block before a
`Write`. Highest leverage in the stack (it reaches the decision before it is
made) and the most dangerous (the agent cannot tell a fabricated enrichment from
a measured one). Nine strategies, each with a measurement behind it — canonical
body: `~/.claude/skills/Touring/references/context-enrichment.md`.

| # | Strategy | The measurement that produced it |
|---|---|---|
| **E1** | Index the field that states **purpose**, not the one that names things | `tantivy search "prior art"` → `art_root` (fuzz shell); yet 96% of 3.881 scripts carry purpose prose nobody read |
| **E2** | Enrichment must be **structurally inescapable**, not persuasive | master commands built and still unused; MUST nudges at conf 0.95 ignored in the emitting session |
| **E3** | **Three sections**, never a ranked list: `prior_art` + `gaps` + `external`, plus a required verdict | a bare list anchors on rank 1; naming the gap is what invites superseding |
| **E4** | **Absence is displayed**, never hidden — and a thin answer reports the corpus size | worst outcome is reusing something broken because it merely ranked well |
| **E5** | **Never fabricate to fill a gap**; expose which corpus answered | `find-code` returned hardcoded `doc_kw_1..5` for every query, in any language |
| **E6** | A **gap claim is an assertion about absence** — compare by word family, never exact | "no candidate mentions professional" while one said "professionally formatted" |
| **E7** | Meet the operator's language **symmetrically** (normalize corpus *and* query) | `search-tools "gerar PDF profissional"` → `No matching tool`; English ranked |
| **E8** | **Derived values, never placeholders** — and boilerplate/stubs are not purpose | a licence banner and `"Command-line interface."` both outranked real artifacts |
| **E9** | Enrichment that **ages silently** is worse than none — invalidate on mtime | a `OnceLock` in a long-lived daemon never sees `portfolio refresh` |

```bash
touring portfolio "<intento>"                     # prior-art por PROPÓSITO (E1+E3)
touring portfolio inspect <arquivo>               # o que o minerador extrai (E4)
touring portfolio verdict "<intento>" --choice extend --why "<razão>"   # o feromônio
touring portfolio history                         # decisões acumuladas
```

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

## ADW — o grafo diz a verdade sobre si (2026-08-19)

Um spec carrega **duas topologias**: a de **controle**, que você declara
(`on_pass`/`branches`), e a de **dados**, que existe de fato (quem interpola
`{{nodes.X.summary}}` de quem). `flow_dataflow()` extrai a segunda; a divergência é o
achado.

| Comando / lint | O que responde |
|---|---|
| `touring adw explain <n> --cost` | o **teto declarado**: chamadas de agente no fan-out cheio, largura máxima, orçamento de timeout |
| lint `fake_waiting` | B roda depois de A, nunca lê A, e **ambos são provadamente read-only** — a espera compra latência, não ordem |
| lint `dead_node` | ninguém lê a saída de um nó cujo produto **é** a saída |
| lint `critique_without_brief` | um painel ligado à entrada julga o que nada produziu ou aprovou |
| `touring adw promote <n> --run <id>` | a library carrega **evidência**, não intenção (`--exempt "<motivo>"` declara a falta) |

Três invariantes que economizam retrabalho: um nó `gate` relê o agente anterior por
`ctx.last_agent`, logo **nunca** é espera falsa; um ramo de `parallel` entrega ao *merge*,
logo nunca é peso morto; e onde o read-only não é **provável** os lints ficam **calados** —
acoplamento por efeito colateral é invisível ao interpolador, e mandar paralelizar esse par
quebraria o fluxo. Declare `readonly = true` num nó `code` para torná-lo analisável.

Kit: 13 fragmentos. Os três modos de paralelismo — `fanout-lenses` (N lentes, concatenado),
`critic-panel` (N críticos sobre 1 alvo, apurado), `worker-critic-pair` (N trabalhadores,
cada um com **seu** crítico). `graph-pack` traz **relações** (`wiring impact` + `index find`
+ `memory moc`), onde `recall-pack` traz texto.

**Acoplamento por construção (M0-M5, 29/08/2026)**: `cross-audit` compõe o `critic-panel` (estreia verde: 3 críticos `tier=mid`, quorum `pass` apurado por código) e `strategy-loop` abre no diamante `ground` (`recall`‖`diagnose`, serial 33,5s → paralelo 25,3s). Nós `agent` journalizam `tier`/`skill` no `node_started`; o SDK orchestrate aceita aliases tipados em `query`/`parallel` e carimba `:par` no origin — KPIs `touring.adw.tiered_agent_share` e `touring.code_mode.parallel_runs` medem a adoção EXECUTADA (`touring kpi -j`).

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
| MCP tools catalog (23 in `tools/list`; ~119 catalogued, invocable by name) | [references/mcp_tools.md](references/mcp_tools.md) |
| **Code Mode recipes** (Reflex #8 cookbook — 5 patterns for `touring_ctx_execute` programmatic tool-calling, 80-96% token savings) | [references/code_mode_recipes.md](references/code_mode_recipes.md) |
| **Code Mode operacional — estratégia por contexto** (apresentação native\|code\|both por escopo, T3-B fusão de turno, classes calibradas, kill switches, efeito medido, guards D8) | [references/code-mode-operational.md](references/code-mode-operational.md) |
| Touring CLI by cluster (7 modules) | [references/touring-cli-overview.md](references/touring-cli-overview.md), [hooks](references/touring-cli-hooks.md), [intelligence](references/touring-cli-intelligence.md), [tasks](references/touring-cli-tasks.md), [rl-quality](references/touring-cli-rl-quality.md), [generate](references/touring-cli-generate.md), [meta](references/touring-cli-meta.md), [assists](references/touring-cli-assists.md) |
| RL stack comparison (Touring vs rsrl) | [references/touring-cli-rl-stack.md](references/touring-cli-rl-stack.md) |
| BugStalker debugging integration | [references/touring-cli-debugging-bugstalker.md](references/touring-cli-debugging-bugstalker.md) |
| Memory hashtag library (facet tags, codetags, MOCs) | [references/memory-hashtags.md](references/memory-hashtags.md) |
| Auto-loaded CLI ranks (constitutional) | `~/.claude/rules/touring-cli-index.md` |
| **Constitution v8.0** (S9 — H3.3) | `~/projects/touring/docs/CONSTITUTION-v8.md` (master, 416L) |
| RFC index (001-005) | `~/projects/touring/docs/RFC-001*.md` · `RFC-002*.md` · `RFC-003*.md` · `RFC-004*.md` · `RFC-005*.md` |
| Constitution v8 audit suite | `~/.claude/audits/2026-05-09-constitution-v8-audit/` (12 scripts, 303 assertions) |
