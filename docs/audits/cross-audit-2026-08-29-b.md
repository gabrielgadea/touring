---
type: AuditReport
title: Cross-audit 29/08 (b) — self-echo, python-inline e a wave ligar-não-construir
description: Auditoria purpose-fidelity dos 3 commits do dia (7450f27, b510455, 47085e0), com prova executada em cada claim; 2 defeitos reais achados e corrigidos no próprio audit.
plan_id: 2026-08-29-ligar-nao-construir
tags: [audit, cross-audit, rl, code-mode, gotcha]
timestamp: 2026-08-29T19:55:00-03:00
okf_version: "0.1"
---

# Cross-audit 2026-08-29 (b) — VERDICT: **PASS com 2 achados corrigidos no próprio audit**

Escopo: os 3 commits do dia — `7450f27` (supressão de self-echo no explore+recall),
`b510455` (python-inline read-only na rajada de inspeção), `47085e0` (wave
ligar-não-construir R1-R6) — 49 file-changes, ~1.7k inserções. Método: 7 fases da
skill, prova executada nunca asserção, sequential-thinking na síntese.

## SCORECARD

| Gate | Resultado |
|---|---|
| DEBT novo (TODO/FIXME/allow nas linhas adicionadas) | **0** |
| REGRA #0 (consumidores dos 11 símbolos novos) | **11/11 wired** (1-10 refs cada) |
| 50-dim nos 8 arquivos-chave | **0.867–0.964, todos ≥ Gold** |
| P0 BLOCK (amostra F2.1/F2.4 no learning.rs) | **Diamond 1.0** |
| Guard D8 texto×executor (`test_code_mode_sdk_section.py`) | **exit 0** |
| Suites | touring-cli 484 · dispatch 1325 · e2e 137+ · intelligence 12 · server 34 · clippy 0 |

## PROVAS EXECUTADAS (cada linha = comando rodado + resultado observado)

| # | Propósito auditado | Prova | Resultado |
|---|---|---|---|
| A | **R2**: evidência sobrevive a restart (o propósito literal) | `daemon-ctl restart` entre 2 leituras | voláteis g1 **1/1 → 0/0**; KPI `inspect_burst_share` manteve **0.5 PASS** lendo `durable_gate_evidence.json` |
| B1 | Aperto python-inline não quebrou o backstop | 5 escritores seriados no hook real | 1ª-4ª passam; **5ª deny G10** com remédio `--lang python` 1:1 |
| B2 | `touring run` zera o ledger da classe nova | read-only → `touring run` → read-only | o 2º read-only passa como 1ª (reset OK) |
| C1 | Contrato de erro do `experiment record` | `--decision banana` | **F-1 ACHADO**: coagido a `keep` em silêncio (pré-fix); pós-fix: `{"error":"unknown decision 'banana'"}` |
| C2 | `resolve --pattern` ambíguo | `--pattern "e"` (13 matches) | erro + `candidates:[…]` — ensina, nunca chuta |
| C3 | `resolve --pattern` único | `--pattern z3-backend-bitrot` | resolveu — e expôs **F-2**: o canal era só-ida |
| D | Fix F-2 se prova no incidente que o motivou | `touring gotcha reopen 35 --why …` | `reopened: true`; stats volta a **resolved: 1** (só o legítimo id=20); reopen de não-resolvido → declarado |
| — | Self-echo (commit 7450f27) | provado na entrega: driver `run_round` real, pós-campanha ECO / pré NORMAL; ledger converged | re-coberto aqui pelas fases 3-4 |
| — | Provas vivas do deploy R1-R6 | `gotcha.resolution` 0→1 · burst_share STUB→0.5 · status 40KB→618B · access_count 2→3 · experiment round-trip best 0.9 | citadas de `47085e0` |

## FINDINGS

**F-1 (CONFIRMADO → CORRIGIDO)** — `cli_experiment_record`: `_ => Keep` coagia
rótulo inválido para o veredito MAIS FORTE (keep atualiza `best_reward`), em
silêncio. Classe: fail-open na direção perigosa. Fix: match exaustivo + erro que
ensina (A5); teste e2e `test_experiment_record_teaches_on_invalid_decision_and_round_trips`.

**F-2 (CONFIRMADO → CORRIGIDO)** — o canal de resolução era só-ida: o PRÓPRIO
audit resolveu por engano o gotcha 35 (z3-bitrot, aberto) e não havia volta sem
SQL manual. Fix (REGRA #0): `touring gotcha reopen <id>` — payload
`{"reopen":true}` no MESMO hook (zero tripwire novo); estreia ao vivo desfez o
próprio incidente. Lição operacional no rodapé.

## ROOT-CAUSE / LIÇÕES

1. **Todo verbo de estado que um humano pode errar precisa do inverso** — um
   canal só-ida transforma engano em estado permanente.
2. **Probe de superfície MUTANTE nunca usa alvo real** — o C3 devia ter usado um
   gotcha sintético; o `--why` documenta, não impede. (O acaso pagou: o erro
   virou a prova do F-2.)
3. **Fail-open tem direção** — `_ => Keep` era fail-open para o lado FORTE;
   entrada desconhecida recusa e ensina, nunca coage.

## RESIDUAIS (declarados, não pendências deste audit)

- Experimento id=3 (probe `xaudit-c1`, keep indevido pré-fix) permanece no log
  append-only — inócuo (0.5 < best 0.9), documentado aqui como probe.
- `pillar_induction_ratio` STUB por design (camada desarmada; evidência durável
  pronta esperando `TOURING_PILLAR_INDUCTION_ARMED=1`).
- Família DSPy aguarda ~30 rejeições num verificador único (gatilho:
  `criterio:p4-dspy-gatilho-familia:2026-08-29`).
- Rotação GEMINI_API_KEY (só Gabriel) · débito release-TEST touring-server.

## PROVENANCE

Commits auditados: `7450f97`→`7450f27`, `b510455`, `47085e0`; fixes deste audit
no commit seguinte a este report. Deploys: `update-touring` ×2 nesta janela,
provas contra o binário/daemon instalados (PID pós-restart). Ledger CCE do tema:
`.touring-explore/ligar-nao-construir-r1-r6-aprendizado.ledger.json` (converged).
