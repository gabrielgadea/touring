# TACO — reflexos, gatilhos automáticos e Phase Protocol

> Movido do `~/.claude/CLAUDE.md` em 11/11/2026 (retro: o steering sempre carregado somava 105 KB e uma tarefa só de prosa não usou nenhum destes itens). O `CLAUDE.md` global guarda o ponteiro; este arquivo é a fonte única.

## Os 11 Reflexos do TACO

| # | Reflexo                | Ação Default                                                                                             | Skip apenas se                       |
| - | ---------------------- | -------------------------------------------------------------------------------------------------------- | ------------------------------------ |
| 1 | **Index First**        | `touring index status`; `touring index rebuild --dir $PWD` se projeto desconhecido                       | Conversa pura sem código             |
| 2 | **Search via Touring** | `touring tantivy search` / `touring index find` antes de Grep                                            | Match exato em 1 arquivo conhecido   |
| 3 | **Recall Memory**      | `touring memory recall "<topic>"` antes de pesquisar do zero                                             | Tema 100% inédito                    |
| 4 | **DAG Decompose**      | `touring decompose create <type> "<desc>"` para 3+ steps                                                 | L0-L1 single-step trivial            |
| 5 | **Blast-Before-Edit**  | `touring ast blast` + `touring pre-edit` (score ≥ 0.8) antes de Edit/Write em código                     | Apenas `.md/.json/.toml/.yaml/.txt`  |
| 6 | **Checkpoint Sempre**  | `touring memory store --tier semantic` por subagent/wave (estado + decisões + aprendizados)              | Single-step trivial                  |
| 7 | **Reward Loop**        | `touring learning reward <tool> <val>` após outcome mensurável                                           | Read-only sem outcome                |
| 8 | **Compute-in-Code**   | A pergunta define a unidade: ≥3 arquivos/fatos → a PRIMEIRA ação é `touring run --file <script>` (1 varredura, só o agregado no contexto); esqueletos R1-R8 prontos: `touring memory query "#kind:snippet #process:code-mode"` | Skip: query exata em 1 arquivo já carregado |
| 9 | **Sandbox-First**      | Rotear execução de código pelo CEG (`run_gateway` X0..X9) antes de qualquer run real                    | Comandos trivialmente read-only (ls, cat, grep) |
| 10 | **OUTER-First**       | `touring adw run strategy-loop --var topic="<meta>" --var scope=$PWD --var bundle=<bundle>` (recall→diagnose→explore-until-dry) **antes** de trabalho substantivo | Conversa/pergunta ou L0-L1 trivial |
| 11 | **Converge-by-Code**  | `loop_converged.py` exit 0 é o ÚNICO "pronto" (path e uso: §DELIVERY CHECKLIST) | Tarefa sem código e sem entregável |

## Triggers Automáticos (TACO engaja sem pedido)

Projeto novo (sem `symbols.db`) → `touring index rebuild --dir $PWD` · tarefa 3+ steps → `touring decompose create` (DAG) · edit `blast_radius > 10` → STOP + plano L3+ + checkpoint · símbolo citado → `touring index find` antes de assumir · erro 2+ vezes → `touring gotcha match` + `memory recall` · daemon degradado → `touring doctor -j` antes de ação stateful · wave concluída → `memory store` + `learning reward`.

## Phase Protocol v6.2

**9 fases sequenciais** (paralelo dentro de cada): 0 HEALTH GATE (cargo check + doctor — **BLOQUEIA**) → 1 SCOUT → 2 ARCHITECT → 3 CONTEXT7 → 4 DECOMPOSE → 4.5 PRE-IMPL AUDIT (anti-FP gate) → 5 ENGINEERS → 6 POST-IMPL AUDIT → 7 DOCS. `sequential-thinking` processa entre fases (deferred — `ToolSearch select:mcp__sequential-thinking__sequentialthinking` em FASE 0).

**Roteamento por nível (CILA)**: L0-L1 SOLO (zero subagents) · L2 = 1→5→validate · L3 = 1→2→5→6→validate · L4+ = todas. **Agent pool**: scouter | architect | engineer | auditor | scriber. **PERMISSÕES — subagents herdam a permission mode do orquestrador** (spawn com as MESMAS permissões que eu — omitir `mode` no `Agent`; NUNCA forçar override mais estreito tipo `acceptEdits`, que faz o subagent pedir Bash a cada comando quando a sessão está em `auto`/`bypassPermissions`). Garantir que EU esteja em `acceptEdits`+ antes de spawnar engineers (edits habilitados por herança). Origem: regressão 2026-07-03 (Gabriel).

**Detalhes**: `~/.claude/skills/Touring/references/{TACO-subagent-rule,VP-Scout-rule}.md` (stubs-ponteiro em `rules/`).
