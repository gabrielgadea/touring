# Touring 4 Pillars — Code Mode · Master CLI · Learning Memory · Intelligence (constitutional, auto-load)

> **Auto-load** (constitutional operational rule) | **Version**: v1.0 | **Date**: 2026-06-29
> **Authority**: Gabriel Gadea | **Origin**: task #6 compounding structure.
> **Empirical lesson (cont.¹⁰)**: I built the master commands and still reached for atomic tools — **adoption does not emerge from availability; it must be actively induced.** This is the *passive* layer; the *active* layer is the graduated `cli_suggester` hook (default-OFF).
> **Companion layers**: hook (`crates/touring-cli/src/cli_suggester.rs` pillar induction) · skill (`~/.claude/skills/Touring/SKILL.md`) · CLAUDE.md (1 reflex pointer).

## The four differentials — reach for these FIRST

| Pillar | First reflex (command) | Replaces (anti-pattern) | Why |
|---|---|---|---|
| **Code Mode** | `touring run --lang python --code '…'` (sandbox, **no MCP**) | shell loop / repeated atomic scan / Read-in-loop | 1 sandbox call replaces N round-trips; 30-200× token compression (Anthropic programmatic tool calling / CodeAct) |
| **Master CLI** | `touring scout/read/map/blast/investigate/guard/audit` | chained `touring index find` + `ast blast` + `wiring …` atomics; `grep -r` for discovery | one master fuses the index/ast/wiring lookups N atomic calls would take |
| **Learning Memory** | **SEMPRE consultar** `touring memory recall "<topic>"` + **SEMPRE registrar** `touring memory store` (planos/progressos/aprendizados/gotchas/decisões) + `touring learning reward` | Read/grep from scratch; não registrar o aprendido | cada lição persistida é o **feromônio ACO** que guia decisões futuras (Reflexo #3) |
| **Intelligence** | `touring ast meta/blast/rust-semantic`, `touring index find`, `touring wiring impact` | guessing structure; editing without blast | the cognitive index answers structure/blast/quality in <10ms |

The four are the standing reflex for code work. Code Mode + Intelligence already fire in the hook (C8 `detect_code_mode`, the read-rust classifier — observed live); Master CLI + Learning Memory are the **gaps** the active pillar layer fills.

> **Code Mode is the umbrella** (Gabriel, 2026-06-29): **Master CLI _is_ Code Mode applied to discovery** — both are CLI-execution **without MCP** that trade N atomic round-trips for 1 call (the same `U(a)=P·V−C(tokens)` win). The four stay listed separately here only so the induction telemetry (`pillar_induction_*`) can measure which differential is under-used; conceptually Master CLI ⊂ Code Mode. CLAUDE.md states the umbrella inline in reflex (1).

> **Corolário ADW (F6.3, 2026-07-20): masters são os nós de código dos ADWs.** Os specs da `adw-library/` (bugfix/feature/audit/explore-plan/scout-perpetuo) invocam `touring memory recall`, `investigate`, `audit`, `blast`, `read`, `explore` e `conflict-check` como nós `code` do runner `touring adw` — **adoção estrutural**: o runner os chama por construção, fechando "adoption must be actively induced" por **afordância** (tese ①: affordance muda `U(a)`; persuasão não). KPI da família: `touring.adw.*` em `touring kpi -j`.

> **A ESSÊNCIA ACO/Touring são Code Mode + Learning Memory** (Gabriel 2026-06-29) — os 2 diferenciais que mais distinguem TACO de um modelo genérico. Code Mode é a execução-em-código que gera o sinal; Learning Memory é o **feromônio** que acumula trilhas de sucesso (cada `memory store` + `learning reward`) e as reusa (`memory recall`). Juntos = o loop _consultar → executar → observar → aprender → registrar → reforçar_ que faz o TACO compor melhoria a cada sessão. Reforçar sempre.

## Injection-density invariant (Gabriel, 2026-06-29) — applies to EVERY context injection AND every answer

**ABSOLUTELY EVERY** context injection (hook `additionalContext`, every nudge, every answer) MUST be:

- **Dense** — high signal-to-token ratio, zero boilerplate (the STR principle, `tool-combination-patterns.md`).
- **Specific** — derived from the REAL input; **never a `<placeholder>` when the value is derivable** (`touring scout AuthValidator`, not `touring scout <symbol>`; for a bash loop, `touring run --lang bash --code '<the verbatim loop>'`, not a python translation with a placeholder). The only honest placeholder is a genuinely non-derivable part (a git memory-recall topic, a future task_id, a not-yet-created symbol). Enforced **across EVERY nudge family** by `every_derivable_nudge_carries_real_value_not_placeholder` (positive assertion: the real value travels) — a STRUCTURAL guard. Lesson (2026-06-29): the earlier narrow per-pillar test let the `code-mode-loop`/`exec-gate`/`find`/`sed` placeholders survive; a quality invariant must be enforced over ALL its instances at once, or it recurs in the next uncovered one.
- **Clear** — MUST / SHOULD / MAY structure.
- **Complete** — the action + alternatives + the rationale.
- **Grounded** — rationale anchored in a NAMED best-practice (Anthropic CodeAct / programmatic tool calling, decision-matrix C-cats, a Reflexo), not a bare "use X".

A generic banner does not induce — it is debt. The cli_suggester `code_mode_command` / `master_cli_nudge` / `learning_memory_nudge` all derive the real argument; the only honest placeholder is a genuinely non-derivable part (an arbitrary loop body), and even then the derivable part (the glob) is carried.

## The active layer — graduated, default-OFF (mirrors F7c)

The `cli_suggester` hook actively nudges Master CLI + Learning Memory. Armed by `TOURING_PILLAR_INDUCTION_ARMED=1` (unset / `0` ⇒ OFF — zero live impact, the shipped default):

```bash
export TOURING_PILLAR_INDUCTION_ARMED=1   # arm the pillar nudges (a human decision)
touring daemon-ctl restart                 # daemon picks up the env (REGRA #19)
```

## The compounding loop — measure → attribute → refine

Adoption is **measured, not assumed**: `pillar_induction_{emitted,followed}` counters → `touring.coupling.pillar_induction_ratio` KPI (`touring kpi -j`) → F7 promote/demote. Per the roadmap thesis ① (**affordance changes `U(a)=P·V−C(tokens)`; persuasion does not**), if the ratio stays low while armed, that telemetry is the evidence pushing from persuasion (this nudge) toward affordance (productization ⑨) — the experiment, not just the nudge.

## Cross-references

| Topic | Local |
|---|---|
| Task→command matrix (C01-C12) | `~/.claude/rules/touring-decision-matrix.md` |
| STR / combination patterns (P1-P10) | `~/.claude/rules/tool-combination-patterns.md` |
| Master command scripts (Layer-3) | `~/.claude/skills/Touring/scripts/` (`discover_symbol`/`discover_workspace`/`analyze_blast`/`read_file`/`investigate`/`pre_edit_gate`) |
| Hook (active layer) | `crates/touring-cli/src/cli_suggester.rs` (pillar induction, `Pillar` enum) |
| Coupling telemetry (F1-F7) | `~/projects/touring/docs/2026-06-27-coupling-telemetry-infrastructure.md` |
| CLI ranks (atomic detail) | `~/.claude/rules/touring-cli-index.md` |

---

_v1.0 — 2026-06-29 | The four differentials are the reflex; the hook induces the two that are under-used; the loop measures whether induction works._
