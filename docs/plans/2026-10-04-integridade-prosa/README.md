---
type: Report
title: Gate de integridade da prosa de saída — instalação em sombra, replay e limpeza
description: Hook PreToolUse que detecta caracteres de alfabeto estranho ao português no texto que sai pelas ferramentas. Instalado em sombra (registra e avisa, nunca bloqueia). O replay pré-registrado não promove o SendMessage a bloqueio (n 25 < 30; Wilson 0,391 > 0,20). A corrupção já commitada foi limpa em 16 arquivos.
plan_id: 2026-10-04-integridade-prosa
timestamp: 2026-10-04T22:45:00-03:00
tags: ["#kind:report", "#domain:hooks", "#artifact:report"]
---

# Gate de integridade da prosa de saída

**Decisão:** Gabriel, 04/10/2026, "recomendação aprovada" no canvas de decisão: opção C (sombra primeiro, bloqueio
do `SendMessage` por critério pré-registrado) mais a D (limpar a corrupção commitada). Pedido de origem: sessão
`analise-ef`, 04/10 às 18:53. Pré-registro: [`pre-registro.md`](/pre-registro.md), commit `259ac249`, sha256
`7fbb91ef979d2c534fa6782dc9252e0c11870899a4cb7ab6acf032bf67d4440d`, escrito antes de qualquer dado.

## O que foi instalado

| Peça | Onde |
|---|---|
| Hook | `~/.claude/skills/Touring/scripts/hooks/prose_integrity_gate.py` (espelho em `client/`) |
| Testes | `test_prose_integrity_gate.py` ao lado: 34 verdes; 7 mutantes, 7 mortos |
| Registro | `~/.claude/settings.json`, PreToolUse, matcher `SendMessage\|Write\|Edit\|MultiEdit\|NotebookEdit`, timeout 5 s (cópia anterior: `settings.json.pre-prose-gate-2026-10-04`) |
| Eventos da sombra | `~/.claude/touring/prose_integrity/events.jsonl` |

- **Modo:** `TOURING_PROSE_GATE_MODE=shadow` (default) registra e devolve um aviso que nomeia caractere, codepoint e
  posição. `block` bloqueia **só** o `SendMessage` (exit 2, com a rota do escape). `Write`/`Edit` nunca bloqueiam.
- **Kill switch humano:** `TOURING_PROSE_GATE_DISABLED=1`.
- **Escape:** no `SendMessage`, o que está entre crases é citação e não conta.
- **Só o introduzido:** em `Write`/`Edit`, conta o que o texto novo tem a mais que o antigo (arquivo em disco, ou
  `old_string`). Editar um teste com CJK legítimo não acusa o que já estava lá.
- **Latência medida:** p50 27 ms num `Edit` limpo, 49 ms num `Write` de 1 MB.
- **Prova ao vivo, nesta sessão, sem reiniciar:** um `Write` real com `Para节省` e um `SendMessage` real (a um
  destinatário inexistente) produziram os eventos e os avisos esperados — 2 ocorrências cada; o `决` entre crases saiu
  como escape. Esses eventos, o do script de limpeza (que carrega os trechos corrompidos como âncoras) e o deste
  próprio relatório (que os cita) são da instalação e **ficam fora da revisão**: todos os eventos da sessão
  `24006143-f691-47fe-b057-e2111fa79e97` em 04/10/2026.

## Replay pré-registrado (04/10/2026)

O mesmo detector sobre todos os `tool_use` dos transcripts em `~/.claude/projects` (controles do scanner 4/4 ok):

| | Chamadas | Detecções | Do MiniMax-M3.1 |
|---|---|---|---|
| Total | 25.024 | 161 | 149 |
| `SendMessage` | — | 25 (4 sessões) | 25 |
| `Write` | — | 96 | 86 |
| `Edit` | — | 40 | 38 |

As 25 detecções de `SendMessage` foram rotuladas pela regra do pré-registro:
[`labels-replay-sendmessage.jsonl`](/labels-replay-sendmessage.jsonl). **20 CORRUPT** (`Recomendo曼`, `o路径 errado`,
`vocêظومة`, `meливca`) e **5 LEGIT**: as cinco citam o próprio defeito numa mensagem que fala dele, sem crases.

**Veredito do critério** (`prose_integrity_gate.py --criterion`): `n 25, LEGIT 5, Wilson-95 superior 0,391,
promote: false`. O `SendMessage` **segue em sombra**: faltam 5 detecções para o mínimo de 30, e a fração LEGIT está
acima do limite de 0,20.

**Candidata a regra nova, não aplicada:** nesta amostra, todo LEGIT estava entre aspas duplas ou numa lista entre
parênteses, e nenhum CORRUPT estava. Tratar aspas como citação separaria a amostra perfeitamente. Exatamente por ter
saído destes dados, ela só pode entrar com pré-registro novo e ser julgada em dado novo — o da sombra.

## Limpeza da corrupção commitada (D)

Varredura de todos os arquivos rastreados: 49 com o sinal. Corrigidos 20 trechos em 16 arquivos, cada um pela
palavra que a frase pedia:

| Arquivo | Antes | Depois |
|---|---|---|
| `crates/touring-storage/src/embeddings/providers/voyage.rs` | `the公共` | `the public` |
| `crates/touring-assists/tests/e2e_assist_pipeline.rs` | `"foo达成;"` | `"foo();"` (14/14 verdes) |
| `crates/touring-cortex/GAP_ANALYSIS.json` | `e误解`, `se同一个` | `e mal-entendido`, `se o mesmo` |
| skill `Touring/references/vp-scout-examples.md` (+ espelho) | `A связывание propuesta` | `A ligação proposta` |
| `scripts/jev_tags_sample.py` | `G1純粋` | `G1 puro` |
| 11 documentos e planos | `Para节省`, `CLI现状`, `выпущен`, `обнаруженный`, `替代品`, `等着`, `代码base`, `トライade`, `单独的`, `+半个`, `优于`, `。`, `门面` | a palavra em português |

**Ficam como estão, legítimos (34 arquivos):** 21 entradas do corpus de fuzz; testes de Unicode
(`日本語テスト`, `fn日本語()`, `'中'.repeat`); os conectivos e terminadores em chinês do detector `erickson`; o
pré-registro (citação entre crases); e `docs/baselines/wiring-pre-refactor-2026-05-11.json`, retrato histórico que
copia o `公共` do `voyage.rs` 42 vezes — corrigi-lo reescreveria o registro do que o código era.

## Fora do alcance, declarado

- A **resposta no chat** não é chamada de ferramenta: nenhum PreToolUse a vê antes de ela aparecer.
- A **classe lexical** (fragmentos em inglês ou alemão no meio do português) passa em qualquer filtro de alfabeto.
- `Write`/`Edit` só avisam; bloquear ou não é decisão de Gabriel, com os dados da sombra.
- O experimento de causa (mesmo prompt, N repetições, 2 checkpoints) exige gasto de API autorizado.

## Juiz

`loop_converged.py --task task_1791163748729547144 --rust-full --json` → **exit 0, `converged: true`** (DAG 7/7,
Platinum 0,926, nenhum P0, órfãos 760 = a baseline da primeira execução nomeada, cargo check + test + clippy
verdes). A cláusula `cross_audit` saiu **N/A** por falta do script do plano, o que não é aprovação. Veredito em
[`judge-2026-10-04.json`](/judge-2026-10-04.json).

## Próximo passo

Revisar as detecções de `SendMessage` que a sombra acumular (fora as 3 da instalação), rotular pela mesma regra,
somar às 25 e rodar `--criterion` de novo. A promoção ao bloqueio é `TOURING_PROSE_GATE_MODE=block` no bloco `env` do
`settings.json`, só quando o critério der `promote: true`.
