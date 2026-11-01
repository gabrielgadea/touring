<!-- touring-4-pillars-provenance.md — movido do rules/touring-4-pillars.md em 08/10/2026 (compactação 65). VERBATIM: a prova da afordança, o bloco ARMADO por ordem do Gabriel, The compounding loop e a tabela de Cross-references. -->

> **A prova da afordância (2026-08-25, cross-audit `docs/audits/cross-audit-2026-08-25.md`)**:
> o enforcement por executor foi provado AO VIVO sem prompt deliberado — 11/11 baterias
> contra o hook real (payload JSON → veredito), a rota do deny **executada** (exit 0),
> counters t3 Δ+1/+1. E o anti-padrão D8 foi pego uma vez mais: o texto dizia *"`ls`/`wc`/
> `sed-n` passam"* enquanto o T3-B os negava na 2ª do turno — o remédio institucional é o
> **guard D8 cruzado** (`scripts/test_code_mode_sdk_section.py`): um teste lê o predicado
> DO EXECUTOR (`CODE_MODE_COLLAPSED_CLASSES` no Rust) e exige que a declaração (a seção de
> sessão) case com ele. Texto e executor reconciliados por teste, não por boa vontade.

> **ARMADO por ordem de Gabriel em 29/08/2026** (durante o deploy 30.4.24).
> Executor: `TOURING_PILLAR_INDUCTION_ARMED=1` no bloco `env` de
> `~/.claude/settings.json` (toda sessão CC e todo hook herdam) + daemon
> reiniciado com a var no ambiente. Expectativa CALIBRADA, não fé: a evidência
> (`protocol-adherence-diagnosis`, tese ①) pesa contra nudges como mecanismo —
> o valor primário do arming é gerar o dado do experimento
> (`pillar_induction_ratio` em `touring kpi -j`); ratio baixo com nudge armado
> é a evidência que promove a migração persuasão→afordância no executor.
> Desarmar (decisão humana): remover a var do settings.json + restart.
> Memória: `decisao:pillar-induction-armado:2026-08-29`.

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
