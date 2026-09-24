---
type: AuditReport
title: Cross-audit — wave N1–N6 (Jev × Touring) + 3 fixes da própria auditoria
description: Auditoria de fidelidade-a-propósito da wave N1–N6 (6 crates/skills/specs) com prova executada ao vivo — 0 órfãos novos, 6 entregas provadas, 3 fixes RED→GREEN, convergência por exit code.
plan_id: 2026-09-23-jev-touring-otimizacao
tags: [cross-audit, n1-n6, jev, evidence]
timestamp: 2026-09-23T23:10:00-03:00
flow: cross-audit (adw)
---

# Cross-audit — wave N1–N6

**Escopo**: todo o código da wave N1–N6 (6 arquivos Rust em 3 crates · 6 scripts Python de skills · 28 specs ADW · `docs/kpi/commitments.yaml`) **+** as correções desta própria auditoria. Convenção: **[E]** evidência executada nesta auditoria (comando + saída) · **[P]** prova prévia da sessão, referenciada.

## FASE 1 — MAP

- **0 órfãos entre os símbolos novos** [E]: `touring wiring orphans -j` → 762 total (baseline), **0** de `flow_compliance*|flow_arm_yield|last_episode_len|snippet_hint*|JournalHint|harvest_candidates|apply_derived_links|resume_ledger|VALID_PURPOSE_CLASSES|lens_yield_totals`. Todo pub novo tem consumidor (sources.rs arms · 3 denies · phase_close main).
- Ciclos: 2 **pré-existentes** (failover mirror `impl_vector_store`; blob de 1236 módulos) — débito estrutural conhecido, não introduzido pela wave. Registrados, fora do blast.
- `snippet_hint_line` indexado (`cli_suggester.rs:4962`, 2 referências) [E].

## FASE 2 — PURPOSE AUDIT (as 6 entregas, prova por entrega)

| Entrega | Prova executada | Veredito |
|---|---|---|
| N1 KPI compliance started-conditional/per-flow | `touring kpi -j` ao vivo [P]: work-outer **1.0** · cross-audit **1.0** · strategy-outer **0.0299** · arm_yield **0.72** | cumpre |
| N2 KPI por episódio + lens_yield | kpi ao vivo: `explore_rounds_to_dry` **2.328** [P] · `touring explore … --status --json` → `lens_yield: {institutional: 10, antistaleness: 5, portfolio: 1}` [E] | cumpre |
| N3 produtor refine cabeado | nó `refine_strategy` executado na prova viva · `plan_refine_iters` **2.0 PASS** ao vivo [P] | cumpre |
| N4 suggest-links + chain edge + resume ruler | phase close N4 → `derived_links` com 3 engine + `loop:…:N4:done extends loop:…:N3:done` [P] · `--uptake` responde JSON [E] | cumpre (ver FASE 5.2) |
| N5 classe de fluxo nas specs + lint | lint sweep 25 specs 0 falhas [P] · `test_adw.py` 247/247 [P] | cumpre |
| N6 hint jornalado + hint no deny + harvest | journal ao vivo: `hint {'key': 'snippet:auto:de58bcd92c27', 'sim': 1.0}` [P] · **deny real disparou com a dica**: `Já há um bloco parecido na escada: snippet:auto:de58bcd92c27 (sim 58%, 4 exec, ○)` [E] · a rota do deny executa: `touring memory recall` devolve o corpo [E] | cumpre |

## FASE 3 — DEBT SCAN

`scan_debt.py` sobre os 5 trees tocados: **0 débito real**. 2 falsos-positivos do scanner (comentários que *citam* "TODO", pré-existentes) [E].

## FASE 4 — HARMONY CHECK

6 P0 dims × 4 arquivos Rust: **Pass/NotApplicable em 23/24** [E] — exceção: `cli_suggester.rs` F2.4 **Warn 0.5** ("secret-related keyword present, no assigned value" — heurística sobre constantes de gate, pré-existente, sem segredo real). Composite do workspace: **Platinum 0.9385** (convergence gate).

## FASE 5 — FIX (3 correções, root-cause registrado, RED→GREEN)

1. **`harvest_candidates` cwd-sensitivo** — achado do audit (0 do dir de scripts × 2 da raiz). Fix em duas passadas após a E2E pegar a sombra errada (`crates/.claude` com 0 entradas): raiz de projeto (`.touring`/`.git`) vence; `.claude` solto é fallback. Testes: nested + stray-shadow, **5/5** [E]. E2E real: 2 candidatos de `crates/touring-cli/src` [E].
2. **Injeção do resume com `session_id: None`** — as 10 injeções do ledger gravaram None (campo cru do payload, ausente em SessionStart/PreCompact/Notification) e `followed` ficou estruturalmente em 0. Fix: `resolve_session` (payload→env→fallbacks) — a mesma resolução do lado da ação. E2E: injeção gravada com `5d8e9b76-…` [E].
3. **zte-probe sem intent/class + xaudit-gates S-8.4** — zte-probe: `intent`+`class=review`, lint limpo [E]. xaudit-gates: escritores são by-design (logs `/tmp/xaudit_*.log` são a entrega); os 6 warnings S-8.4 ficam **advisory documentados** — suprimir seria reduzir, e um gate humano quebraria o propósito determinístico do flow.

## FASE 6 — E2E PROOF

- Suites completas pós-fix: **257 + 139** (loop-engineering scripts + hooks) · 784 (touring-code) · 576 (touring-cli) · 1636 (touring-server) · 247 (adw) · 19 (explore) · 15 (plan_refine) — tudo verde [P/E].
- Convergência da wave: `loop_converged.py` **exit 0** (8 cláusulas) [P].
- Afordância provada por payload real, não demo: rajada de 2 python-inline → deny com programa fundido + snippet trusted + remédio executável [E].

## Veredito

**PASS** — 0 órfãos novos, 0 débito real, as 6 entregas cumprem o propósito documentado com prova executada, 3 achados da própria auditoria corrigidos com RED→GREEN. Advisory: F2.4 Warn (heurística pré-existente), 2 ciclos pré-existentes, 6 warnings S-8.4 do xaudit-gates (by-design).

_Executor: sessão 5d8e9b76 · suites: 257+139+784+576+1636+247+19+15 verdes · mirror client/ sincronizado._
