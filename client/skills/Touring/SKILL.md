---
name: touring
description: Master integration skill for the Touring code intelligence stack (CLI + daemon + 23 MCP tools in the `tools/list` handshake; ~119 more are catalogued and invocable by name via `tools/call`). Use ALWAYS before editing code, creating modules, refactoring, code review, writing tests, or planning architecture in projects under ~/projects/touring/. Invoke when the user mentions Touring, TACO orchestration, file metadata, blast radius, VGP verification, wiring orphans, code generation, MCTS planning, RFC-100 diagnostics, or any of the touring-* subagents (scouter, architect, engineer, auditor, scriber). Provides pre-edit safety gates, symbol verification, RL-backed suggestions, and 120+ CLI commands for code intelligence and quality tracking.
---

# Touring — Master Integration Skill

> **Compactação 65 (08/10/2026).** Este arquivo ficou abaixo do corte de
> 20.000 caracteres do Claude Code, para que o modelo veja o procedimento
> inteiro depois de uma compactação. As tabelas de comandos, o cheatsheet e a
> doutrina de contexto foram movidos **verbatim** para
> [references/touring-cli-e-contexto.md](references/touring-cli-e-contexto.md) —
> nenhum conteúdo foi perdido, apenas movido.

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

> **Motor VGP (09/10/2026)** — a verificação mora no EXECUTOR: `touring vgp check` + hooks (Edit/MultiEdit/Write e
> Python inline do Bash) checam o conteúdo proposto contra o projeto real (camada 1 nativa + `pyright-langserver` do
> projeto). Política P4: código que roda agora (inline, `/tmp`) nega símbolo inventado; arquivo do projeto nega só
> alvo externo e avisa o resto (RED de TDD é legítimo); forma do dado e atributo dinâmico só avisam. Modo
> `[vgp] modo = "sombra"|"ativo"` no `.touring/touring.toml`. Manual: `~/projects/touring/docs/vgp-motor.md`.
> Ao ESCREVER spec/plano que cita API, o motor não vê o texto — aí vale o passo manual abaixo.

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
> **Caminho corrigido (08/10/2026):** `~/.claude/skills/touring-elite/references/quality/D01_complexity.md (um arquivo por dim, D01..D52)` — o caminho relativo acima nao resolve nesta skill; o arquivo real esta na skill indicada.

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
12. **Verification before completion (MUST E — TDD-gate universal)** — NO COMPLETION CLAIMS WITHOUT FRESH EVIDENCE (added 2026-09-01, Rank #1 do plano `docs/plans/2026-09-01-skill-aprimoramento/`). Before stating "done", "fixed", "passes", "ready", "ship", or any success synonym, you MUST have run the verification command in **this turn** and seen the output. Red-green cycle for regressions: write test → run (pass) → revert fix → run (MUST FAIL) → restore → run (pass). Source: obra `superpowers:verification-before-completion` (Iron Law) cross-pollinated; closes universal gap (E) RED-GREEN-REFACTOR across 7 skills (Marcel Point #1 do Gabriel — single MUST idêntico, single commit). Rationalization table + escape patterns: [references/verification-before-completion.md](references/verification-before-completion.md).

---



## Reference Map

Operational depth (consult on demand):

| Topic | File |
|-------|------|
| **CLI por tier (TIER 1-9), cheatsheet, scripts Layer-3, doutrina de enriquecimento, subagent pool, melhores praticas, janela de contexto e o grafo ADW** | [references/touring-cli-e-contexto.md](references/touring-cli-e-contexto.md) |
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
> **Caminho corrigido (08/10/2026):** `~/.claude/skills/TACO-skilling/references/touring-integration.md` — o caminho relativo acima nao resolve nesta skill; o arquivo real esta na skill indicada.
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
