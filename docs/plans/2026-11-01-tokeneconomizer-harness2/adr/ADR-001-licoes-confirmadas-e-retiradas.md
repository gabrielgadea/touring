---
type: ADR
title: "ADR-001 — Lições mineradas passam a ter ciclo de vida: candidata, confirmada, retirada"
description: "As lições de transcrito entram por um único episódio e nunca saem; adotar confirmação entre sessões e retirada por contradição, no molde das micro-skills do TokenEconomizer."
timestamp: 2026-11-01T18:00:00-03:00
status: proposta
---

# ADR-001 — Lições mineradas passam a ter ciclo de vida

## Contexto

- **Nosso mecanismo (FATO 1.0):** `crates/touring-server/src/ingest/transcript_miner.rs:421`
  (`extract_error_resolution_pairs`) transforma UM par erro→resolução testemunhado em lição; a chave
  é `(tool_name, 200 chars do erro)` (`:499-506`) e uma recorrência noutra sessão é descartada como
  duplicata — o banco não sabe quantas sessões viram o mesmo fato. A injeção é
  `crates/touring-cli/src/cli_suggester.rs:2063` (`collect_memory_lessons`).
- **Ausência provada (FATO 1.0):** busca `contradict|superseded_by|invalidate_lesson|stale|expire`
  em `crates/` e hooks não acha retirada de lição; `decay|half_life|last_seen|ttl` em
  `crates/touring-server/src/ingest` não acha nada.
- **Medido (F4a, 250 transcritos, `medicao/f4a_lessons.py`):** 530 chaves; **499 (94%) vistas numa
  sessão só**; 20 (3,8%) confirmadas por ≥ 3 sessões. Com o erro normalizado: 402 chaves, 364 (90,5%)
  de sessão única, 21 (5,2%) confirmadas.
- **Medido (F4b):** **9.367** lições no `memory.db` do projeto, e em 250 sessões o bloco
  `Past failure [` apareceu em 2 sessões (29 linhas, 2.300 B, 2 chaves). Estoque grande, entrega quase
  nula — o canal não está nem ajudando nem custando; está inerte.
- **Externo (FATO 1.0, lido):** `tokeneconomizer/microskills/lifecycle.py:15-21` — candidato vira
  ACTIVE com `K_PROMOTE=3` confirmações em tarefas que editaram arquivos diferentes; score com
  recência `GAMMA=0.8` e contradição pesando `LAMBDA=5`; `HARD_KILL` numa contradição limpa para
  fato de alto risco; injeção só com `FIRE_GATE=0.40` em `FIRE_MIN_N=4` tarefas. Teto de injeção:
  4 fatos / 150 tokens (`config.py:148-151`). Ablação no Claude Code (paper, Tabela 2a): micro-skills
  sozinhas −11,8% (Sonnet) / −14,3% (Opus) de custo.

## Opções

1. **Manter** — estoque cresce, entrega segue ~0.
2. **Copiar o módulo deles** (APROVEITAR) — Python, acoplado ao formato de trace deles e a side-calls
   de modelo; nosso minerador é Rust e determinístico.
3. **Ciclo de vida próprio** (INSPIRAR + CORRIGIR): contador de sessões distintas por chave
   normalizada, estado `candidate|active|retired`, retirada quando a mesma ação passa a dar certo
   sem a resolução registrada (contradição observável no próprio transcrito), e injeção condicionada
   ao seguimento medido (ver ADR-002).

## Decisão recomendada

**Opção 3** (confiança 0,8 — INFERÊNCIA: o ganho no nosso ambiente não está medido; o defeito está).
Normalizar a chave (caminhos, números, hex) antes do hash, contar sessões distintas, promover com
k=3, retirar por contradição (λ alto, falha para o lado da retirada) e por recência.

## Reversibilidade

Alta: o estado novo é coluna/tag na memória; a injeção atual pode continuar lendo tudo enquanto o
experimento roda (sombra).

## Como medir

Pré-registro no BACKLOG item B1: fração de lições injetadas que o turno seguinte segue
(`injection_follow_ratio` restrito à família `past-lessons`) e bytes por turno, antes × depois,
com n mínimo declarado.
