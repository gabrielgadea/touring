---
type: ADR
title: "ADR-004 — Detector de espiral por trajetória como sinal (sombra), composto com a memória"
description: "Nossos gates julgam chamada a chamada e não interrompem espirais (45 de 47 turnos marcados já tinham deny); adotar um detector de trajetória em sombra, que nunca bloqueia sozinho."
timestamp: 2026-11-01T18:00:00-03:00
status: proposta
---

# ADR-004 — Detector de espiral por trajetória

## Contexto

- **Nossos guards (FATO 1.0):** G6/G1/G10 em `crates/touring-cli/src/cli_suggester.rs` (rajada,
  repetição após deny, exec-burst), cap A14 de 3 tentativas (`client/rules/code-elaboration-directives.md:24`),
  `loop_stop_guard.py` (artefatos do fluxo, não progresso). Granularidade: a CHAMADA.
  Detector de estagnação por trajetória existe só dentro do ADW e é opt-in
  (`client/skills/Touring/scripts/adw.py:2800-2806`, `stagnation_rounds`). Em sessão interativa:
  ausente (busca `spiral|edit_churn|stagnation|trajectory.*stuck`).
- **Externo (FATO 1.0):** `tokeneconomizer/loopguard/rules.py:20-43` — conjunção de ≥ 20 turnos,
  churn de edição (≥ 4 num arquivo ou ≥ 3 arquivos), ≥ 6 erros, ≥ 20% de erros repetidos, além do
  envelope p90×1,25 das tarefas resolvidas; juiz de LLM com confiança ≥ 0,7, no máximo r=1 redirect
  por turno (`turn.py:334`), backstop 2,5× do envelope. Ablação: guard sozinho −7,1% / −5,4% de custo,
  e no Opus o **menor Pass@1** (72,6 contra 74,0) — o guard é o mecanismo de menor ganho e o único com
  sinal de dano à qualidade.
- **Medido (F4c, `medicao/f4c_loopguard.py`):** 715 turnos com tools; 216 com ≥ 20 chamadas; o
  detector heurístico marcaria **47** (21,8%); **4.141 chamadas (19,5%) vêm depois do ponto de
  detecção**; **45 dos 47 já tinham recebido um deny do Touring antes** — os gates disparam e a
  trajetória segue. Limite: sem rótulo de tarefa resolvida o envelope não é aplicado, e nossos turnos
  são longos por desenho (mediana 142 chamadas nos marcados) → é TETO de oportunidade, não desperdício
  provado.

## Opções

1. Nada (gates por chamada bastam) — refutado pelo 45/47.
2. Copiar o juiz de LLM — side-call pago a cada 5–20 chamadas, e o próprio paper mostra queda de Pass@1.
3. **Detector determinístico em sombra** (INSPIRAR): calcula a assinatura por turno, registra, não
   injeta nada; depois de calibrado, um único aviso por turno que **traz a lição da memória** para o
   erro que se repete (POTENCIALIZAR: memória × guard — o aviso deixa de ser genérico).

## Decisão recomendada

**Opção 3, fase sombra primeiro** (confiança 0,65 — ESPECULAÇÃO quanto ao ganho; FATO quanto à
lacuna). Calibrar os limiares nos NOSSOS turnos (os deles vêm de SWE-Bench) e definir "resolvido"
offline antes de qualquer aviso.

## Reversibilidade

Sombra: total. Aviso: kill switch por env.

## Como medir

BACKLOG B4: concordância do detector com rótulo humano numa amostra pré-sorteada; depois, A/B de
chamadas por turno e de retrabalho.
