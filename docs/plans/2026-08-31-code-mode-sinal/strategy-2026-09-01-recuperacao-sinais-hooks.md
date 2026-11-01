---
type: Strategy
title: "Recuperação e continuidade — sinais dos hooks do touring"
description: "Recuperação verificada por evidência das frentes code-mode-sinal F1-F6 e complementacao-hooks (ambas entregues) + estratégia de continuidade em 3 rotas, aguardando HUMAN GATE"
plan_id: 2026-08-31-code-mode-sinal
content_blake2b: cffc516c57b1a47d44871ddbe6112015
tags: ["#kind:decision", "#domain:hooks", "#process:strategy-outer", "#status:pending-human-gate"]
timestamp: 2026-09-01T16:36:34.215302-03:00
---

## Recuperação — estado verificado

O trabalho "sinais dos hooks do touring" são DUAS frentes irmãs, ambas ENTREGUES e verificadas por evidência viva neste turno (01/09/2026):

**Frente 1 — code-mode-sinal F1-F6** (DAG `task_1788196388043002698`, bundle `docs/plans/2026-08-31-code-mode-sinal/`):
- DAG 6/6 `done` (verificado via `touring decompose ready` — 0 ready, 6 done).
- Entregas: `crates/touring-code/src/sdk.rs` (8 hooks canônicos + SignalReport, 330L), `journal.rs` (parser fail-soft do run_journal, 381L), `scripts/gen_sdk.py` (emissor CI sem toolchain Rust), `sdk_signal_mirror.rs` (sink JSONL append-only, 373L), `record_hook_call` injetado no template Python do `--orchestrate` (`crates/touring-server/src/cli/run.rs:298`), 4ª regra `signal_use` no BestPracticesGate (`crates/touring-quality/src/builtins/best_practices.rs`, Warn-severo por design), KPIs `code_mode_signal_use` + `code_mode_economy_ratio` (`crates/touring-cli/src/cli/kpi.rs:478`) + `examples/kpi_f6_smoke.rs`.
- Commits: `4cb6470` (código F1-F6), `52023cb` (fix deploy bin stale — daemon linka touring-cli estático), `8d4cda0` (toolchain 30.4.28 propagação nativa), `148fc74` (docs co-evolução F1-F6, REGRA #21).
- Deploy: toolchain 30.4.28 nativa em `~/.touring/toolchains/30.4.28/`, 3 projetos pinados (touring/analise/konverter) verificados por comportamento (27/26/26 passed, 0 fail).
- Evidência viva (medida neste turno): `touring kpi -j` → `code_mode_signal_use {used:3, total:8, ratio:0.375, total_calls:9}`; mirror `~/.claude/touring/sdk_signal_mirror.jsonl` com 9 linhas (seed dos examples F3); relatórios canônicos `sdksignal-report-f{2,3,4,6}.json` no bundle; phase reports em `phases/` + knowledge abstracts em `knowledge/`.

**Frente 2 — complementacao-hooks** (DAG `task_1788202012024662508`, bundle `docs/plans/2026-08-31-complementacao-hooks/`):
- DAG 6/6 `completed` (F1-specs-15-handlers, F1.5-criar-4-subcmds-faltantes, F2-benchmarks-latencia, F3-registro-settings, F3-implementar-signal-layer, F4-testes-gate-kpi).
- Entregas: 15 specs H1-H15, SignalLayer trait substituindo wrappers do settings.json, 4 KPI gates, composite 0.9281 Platinum. Código em `74b75b0` e commits anteriores da branch.

**VEREDITO DA RECUPERAÇÃO: nada foi perdido.** Todo o trabalho está commitado na branch `safety/2026-08-31-audit-closure`, deployado nos 3 projetos e com KPI vivo no binário de produção. O único artefato não-commitado relacionado à frente é o diagnostic OKF gerado pelo próprio OUTER desta sessão.


## Pendências — cadeia causal

O delta entre "entregue" e "cumprindo o propósito", em cadeia causal:

1. **[GARGALO] `signal_use` ratio 0.375 < 0.80** — o critério `01_signal_use` do F6 FALHA porque o mirror tem apenas 9 linhas seedadas pelos examples do F3, não por uso real. A memória do F3 é explícita: "PostToolUse handler agora sabe COMO escrever; falta WIRAR (deployment, não código)". O CLAUDE.md do workspace já nomeia a próxima wave: "PostToolUse wirar em satélites + measurement adoption (F6 secondary KPIs)".
2. **F6 secondary KPIs null** (`token_reduction_pct`, `rounds_reduction_pct`) — aguardam adoção medida; bloqueados por (1).
3. **Promoção F5 Warn→Block** — decisão de política de Gabriel + 7 dias estável ≥ 0.80; hoje indecidível porque (1) impede o 0.80.
4. **PR review → main** — a branch `safety/2026-08-31-audit-closure` acumula o trabalho de sinais (commitado) + o cohort skill-aprimoramento de 01/09 (parcialmente não-commitado: 9 modified em `client/` + untracked `docs/plans/2026-09-01-*`).
5. **Step 5.5 do propagate** — 9/39 asserts em warning não-bloqueante (stale cache do daemon); verificação daemon vs source timestamps pendente.

O finding da lente portfolio confirma: nenhum prior-art cobre este intento (11.502 registros varridos) — o intento correto da continuação é a WAVE DE WIRING, não uma recovery.


## Estratégia — 3 rotas

Três rotas, ordenadas por desbloqueio causal (A e B são compostas — A não depende de B):

**ROTA A (recomendada) — Wave PostToolUse-wiring**: wirar o PostToolUse REAL para alimentar o sink `sdk_signal_mirror.jsonl` em uso de produção (incluindo satélites analise/konverter), fazendo `signal_use` subir de seed-de-example para contagem real. Critério de pronto: ratio medido por uso real (não examples) + 7 dias de observação → a decisão F5 Warn→Block torna-se decidível com dado, não fé. Escopo estimado: L2-L3 (o código existe; é deployment wiring + verificação comportamental).

**ROTA B — Higiene de branch**: PR review → main do trabalho commitado; decidir se o cohort skill-aprimoramento (não-commitado) entra no mesmo PR ou em commit/PR separado.

**ROTA C — Observar**: as duas frentes estão entregues e estáveis; não fazer nada agora, deixar o mirror acumular organicamente e voltar em 7 dias para a leitura.

Recomendação: **A**, com B na sequência (ou em paralelo, pois B é só git/review). C é legítima mas deixa o critério 01 do F6 falhando por tempo indefinido — o mirror só cresce se o PostToolUse alimentar de verdade.


## Incógnitas (grilling)

Incógnitas que mudariam a estratégia (grilling — a resolver no HUMAN GATE antes de plan/decompose):

1. A wave de wiring (Rota A) deve tocar os satélites (analise/konverter) já na primeira fase, ou primeiro só o touring e propagar depois de provado?
2. O PR → main (Rota B) inclui o cohort skill-aprimoramento de 01/09 ou apenas o trabalho de sinais? (O cohort ainda tem arquivos não-commitados na branch.)
3. A política de promoção F5 (Warn→Block) espera os 7 dias estáveis ≥ 0.80, ou Gabriel quer um gate manual de decisão antes disso?
