---
type: ResumePointer
title: RETOMAR AQUI — Touring Autoresearch (P0/P1 fechados, P2 pronto)
description: Estado em 2026-08-25 20:50 — estratégia aprovada por Gabriel (P0–P4 completo revisado), DAG criada, P0 e P1 done com prova medida.
plan_id: 2026-08-25-autoresearch-rl-intelligence
tags: [loop, resume, autoresearch, rl]
timestamp: 2026-08-25T20:50:00-03:00
okf_version: "0.1"
---

# RETOMAR AQUI — 2026-08-25 (rodada 2 + P0/P1 entregues)

## Estado: DAG `task_1787700607622354408` · P0 done · P1 done · **P2 ready**

### Decisão tomada (HUMAN GATE cumprido)
Gabriel aprovou **"P0–P4 completo, revisado"** após uma segunda rodada exaustiva de exploração
(exigência dele: infraestrutura + fontes externas + **Context7**).

### O que a rodada 2 mudou (ler `research-2026-08-25-rodada-2.md`)
- **O laço de crédito já existia inteiro e tinha ZERO chamadores** — `ledger_credited_total = 0`
  na vida do daemon. Os 1,7% não eram aprendizado lento: era um laço nunca fechado.
- 3 afirmações da rodada 1 caíram: DSPy **existe** no Rust (`touring-cortex/handlers/dspy_compile.rs`);
  `loop_phase_close` **já tinha** `credit_recalls()` sem alimentação; `factory.py` **já premia** o router.
- Externo: Context7 `/nousresearch/hermes-agent-self-evolution` (laço de 6 passos DSPy+GEPA) e
  `/websites/dspy_ai` (métrica = `dspy.Prediction(score, feedback)`, piso 30–300 exemplos);
  survey MSR ~300 papers: **autonomia só onde há verificador determinístico independente**.

### Entregue e provado
- **P0**: `Ledger::drain_pending()` + `touring memory credit --all-pending` + fallback automático
  no `loop_phase_close` + `deposit_run_outcome()` em `adw.py::execute()` (reward por run + crédito).
  Medido: `outcome_reward` **147/8.641 = 1,70% → 225/8.688 = 2,59%** (+52% relativo).
- **P1**: D1 (o nó `explore_round` era estruturalmente incapaz de iterar), D2 (slug sem dobra de
  acentos bifurcava o ledger), D3 (gate aceitava qualquer ledger fresco) — 16 guards novos.
- Gates: **3.450 testes Rust verdes**, clippy exit 0, `adw lint` sem erros, espelho `client/` CLEAN.

### P2 — FECHADO (decisão (b) de Gabriel implementada e provada)

O braço `code_mode` ganhou controle da apresentação atrás de `TOURING_CODE_MODE_ARM_ARMED`
(default-OFF), com promoção por evidência medida. Invariantes fixadas por teste: desarmado é
byte-idêntico ao anterior; declaração humana nunca é sobrescrita; piso de 20 amostras **e** mínimo
de 2 braços elegíveis **e** margem ≥ 0,05; evidência corrompida ⇒ política calada.

Prova ao vivo e durável: deny → `{code:{offered:1}}` em disco → rota tomada →
`{code:{offered:1,followed:1}}`.

Sub-fases: `P2b` decisão (done) · `P2c` fonte da oferta corrigida por medição (done) ·
`P2d` KPI lendo a mesma fonte (implementado, deploy pendente) · `P5b` economia (idem).

### O que mudou por observação de Gabriel

1. **Contadores voláteis** — `t3_turn_first_passed` caiu 2→0 num restart. Com piso de amostra e
   três deploys por sessão, a promoção nunca aconteceria. A decisão passou a ler disco.
2. **A rota sancionada é isenta de todo gate** — `scan_class_of` não reconhece `touring run`, então
   N programas triviais somam N adoções e zero avisos. Resolvido no `P5b`: a economia virou o
   **custo** do braço (0,5 para casca, 1,0 para fusão) + KPI `touring.code_mode.economy_ratio`.

### Higiene do disco (26/08 00:46)

46% → 34% (129,5 GB liberados por `safe-clean.sh incremental`). Três defeitos corrigidos na
ferramenta: aviso que prometia por todos os modos o que só `incremental` cumpre; `sweep` (que o
**cron semanal rodava**) agora aborta com daemon vivo; scripts movidos para `scripts/` com symlink
(estavam fora de controle de versão). Detalhe: memória `sweep-apaga-o-binario-do-daemon`.

### Estado da DAG (8 de 11 fechadas)

`P0` `P1` `P2` `P2b` `P2c` `P2d` `P3` `P5b` **done** · `P4` `P5` `P3b` **pending**.

### P3 — a primeira campanha real, com número

Corpus **congelado** de 5.338 comandos Bash reais (22 sessões, 40 transcripts). Harness de replay
usando os predicados **reais** e estado local. Curva medida sobre 1.099 inspeções:

| limiar | colapsadas | % | sim. média |
|---|---|---|---|
| 0,0 | 1007 | 91,6% | 0,219 |
| 0,4 | 107 | 9,7% | 0,551 |
| **0,5** | **77** | **7,0%** | 0,599 |
| 0,6 | 50 | 4,5% | 0,645 |
| 0,7 | 10 | 0,9% | 0,803 |

O limiar 0,5 colapsaria 77 inspeções reais; há joelho entre 0,6 e 0,7; abaixo de 0,4 o gate passa a
fundir comandos genuinamente diferentes. **7 variantes no `variant_archive`**, 3 marcadas
`--terminal`, 4 elegíveis. E o keep/discard demonstrou com número por que o escalar sozinho não
serve: maximizá-lo empurra o limiar para 0.

Rodar de novo:
```bash
python3 eval/autoresearch/extract_corpus.py --out eval/autoresearch/corpus.json --limit 40
cargo test -p touring-cli --lib autoresearch_replay -- --ignored --nocapture
```

### Por que o loop não converge hoje

`P4` está bloqueado por **dado, não por implementação**. O desbloqueador está no ar
(`adw.py::_store_gate_rejection` grava toda reprovação de gate como caso `--reward 0.0`), mas os
rótulos precisam acumular com uso real. Critério de partida:

```sql
SELECT COUNT(*) FROM memory_entries WHERE outcome_reward < 0.5;   -- hoje: 16
```

`P5` (consolidação + `loop_converged --rust-full`) depende de `P4`. `P3b` (embalar o research loop
como ADW) é independente e implementável a qualquer momento.
