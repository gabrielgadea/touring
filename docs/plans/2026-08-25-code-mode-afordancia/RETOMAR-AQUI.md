<!-- OKF document -->
---
okf_version: "1.0"
type: ResumePoint
title: "Retomar aqui — afordância de code mode"
description: "Estado exato ao fim da sessão de 25/08/2026: P0 e P1 fechados e provados vivos, P2 escrito e testado mas incompleto, P3 e P4 pendentes."
plan_id: 2026-08-25-code-mode-afordancia
tags: [code-mode, afordancia, retomada, handoff]
timestamp: 2026-08-25T12:45:00-03:00
authority: Gabriel Gadea
dag: task_1787656862986274376
---

# Retomar aqui

## Em uma frase

O nudge deixou de exortar e passou a **entregar o programa** que substitui as N
chamadas (emenda do Gabriel, 25/08). **TODAS as fases estão fechadas e a DAG
CONVERGIU** (loop_converged --rust-full, unmet:[], 25/08 ~17h): P0 reparo, P1 SDK
1× + nudge-programa, P2 escopo + calibração + prefixo por-comando, P3 fusão de
turno T3-B (prova de aceitação viva), P4 A/B por braços + memória inflada
substituída. **Plano CONCLUÍDO.** O que resta é follow-up (P5/T3-A hold-and-fuse
só se a telemetria t3 justificar; A/B ao vivo se o Gabriel preferir ao corpus).

## O primeiro comando da próxima sessão

```bash
cd ~/projects/touring
touring memory recall "afordância code mode P2"
touring decompose ready task_1787656862986274376
python3 ~/.claude/skills/loop-engineering/scripts/loop_converged.py \
  --task task_1787656862986274376 --scope /home/gabrielgadea/projects/touring \
  --bundle docs/plans/2026-08-25-code-mode-afordancia --json
```

O marcador do loop já está `active` neste DAG, então o Stop hook continua
arbitrando. Bundle: `docs/plans/2026-08-25-code-mode-afordancia/`.

## Estado por fase

| fase | estado | evidência |
|---|---|---|
| **P0** reparo do caminho preferido | **done** | `phases/P0.md` |
| **P1** SDK 1× + nudge entrega o programa | **done** | `phases/P1.md` |
| **P2** `TOURING_CODE_MODE` por escopo | **done** | `phases/P2.md`, `phases/P2.3.md`, `phases/P2.4.md` — 402 testes, prova viva do prefixo |
| **P3** fusão automática da rajada | **done** | `phases/P3.md` — T3-B deployado, prova viva (grep3 fundida) |
| **P4** medição A/B + memória inflada | **done** | `phases/P4.md` — memória substituída, telemetria viva |

## O que foi completado no P2 (retomada 25/08 tarde)

Os dois itens que faltavam estão **feitos e deployados**:

1. **Prefixo `TOURING_CODE_MODE=<v>` no próprio comando** — nível mais externo da
   resolução, provado vivo: `TOURING_CODE_MODE=native grep …` passa num projeto
   `code`; sem prefixo, o deny segue. 4 testes novos (relaxa/aperta/inválido/aspas).
2. **Docstring honesto** — "prefixo do comando → env do hook → alias → projeto →
   default", com a razão medida: o shell da Bash tool e o hook são processos
   IRMÃOS, exportar num é invisível ao outro. O texto do deny agora ensina a via
   que funciona (prefixo), não a que falha (export).

Mais, da mesma retomada: flaky gateway::metrics morto (7 testes serializados +
guard no CI), `loop_phase_close.py` resolve subtask id (mutação 0→1→0), calibração
P2.3 + efeito P2.4 com relatórios OKF próprios.

## P3 — o desenho já fechado (é só implementar)

Responde à crítica de origem: *"seu teste não provou nada, você o criou de forma
intencional"*. O critério de aceitação é **emitir K chamadas atômicas e receber
UM programa que eu não escrevi**.

**Variante B (first-wins, fold-the-rest)** — a recomendada para primeiro:

- A **primeira** chamada da rajada executa intacta (seu resultado é real, nada
  se perde).
- As **K−1** irmãs do mesmo turno são negadas com UM programa derivado dos
  comandos verbatim (usar `fuse_burst_program`, que já garante comando inteiro
  ou nenhum).
- Custo: um round-trip. Ganho: N→1, e a derivação foi do harness.

**Detecção do "mesmo turno"**: por **timestamp de chegada**, não por
`PostToolUse`. Chamadas paralelas de uma mesma mensagem chegam com milissegundos
de diferença; chamadas sequenciais são separadas pela latência de inferência
(segundos). Janela sugerida: 300 ms, tunável. Um anel por sessão com
`(timestamp, classe, cmd)` basta — o padrão dos demais gates (`burst_ledger`,
`g3_read_files`) já usa `moka` com TTL.

**Ordem**: G9 (rajada do turno) **antes** do G1 (rajada de 180 s) — o sinal do
lote é mais específico.

**Variante A (hold-and-fuse)**: segurar a resposta da primeira por ~25 ms,
coletar as irmãs, reescrever a primeira via `updatedInput` e negar o resto. Zero
round-trip extra. Só depois que a telemetria da B provar frequência e acerto —
se o programa fundido falhar, perdem-se as K de uma vez.

## P4 — o que medir, e a dívida a quitar

- A/B `native` × `both` × `code` sobre as mesmas tarefas, reportando **payload,
  comando e nudge separados** (as três grandezas que o benchmark anterior
  misturou), mais latência e **corretude**.
- **Dívida**: a memória `prova:code-mode-eficiencia-medida:2026-08-25` carrega
  84,8% de redução de contexto — número **inflado**, porque o benchmark debitou
  as 1.196 injeções de hook só do braço atômico. Deve ser **substituída**, não
  corrigida na margem. `scripts/bench_code_mode_vs_atomic.py` tem o mesmo vício.
- Nenhuma reivindicação de economia antes do número — nem a fonte do DeepSeek
  reivindica (*"makes no unconditional-savings claim"*).

## Arquivos tocados (todos commitados na branch `safety/`)

| arquivo | o quê |
|---|---|
| `crates/touring-cli/src/cli_suggester.rs` | `fuse_burst_program`, `CodeModePresentation`, `project_presentation`, `CODE_MODE_COLLAPSED_CLASSES`; guard anti-recursão em `code_mode_kind`; `bash_code_mode_command` sem truncagem; `loop_code_mode_command`+`loop_glob` REMOVIDAS |
| `crates/touring-cli/src/cli_suggester_tests.rs` | 12 testes novos em 3 módulos, todos provados por mutação |
| `crates/touring-ceg/src/gateway/gate.rs` | `invoked_binaries`, `call_site_args`, `first_string_literal`, `command_name` — deriva o binário real em vez de pedir `Run(any)` |
| `crates/touring-ceg/src/capability/builtins.rs` | `READ_ONLY_BINARIES` (22) concedidos no perfil `Sandboxed` |
| `crates/touring-ceg/src/gateway/summarize.rs` | `elided_lines` + `truncated` honesto |
| `crates/touring-cli/src/cli/handlers/decompose.rs` | campo `subtask_missing` |
| `scripts/hooks/code_mode_sdk_section.py` | **novo** — cópia versionada do hook vivo |

## Fora do repo (⚠ não versionado pelo git deste projeto)

| caminho | estado |
|---|---|
| `~/.claude/hooks/code_mode_sdk_section.py` | vivo; cópia idêntica em `scripts/hooks/` |
| `~/.claude/settings.json` | hook registrado ADITIVAMENTE; backup em `settings.json.bak-20260825T090859`. Reverter = remover a entrada `code_mode_sdk_section` |
| `~/.claude/skills/loop-engineering/scripts/loop_phase_close.py` | `--subtask`, `validate_facts`, `update_dag` honesto. Espelhado em `client/` (CLEAN) |
| `~/.claude/projects/…/memory/` | 2 memórias novas, indexadas em `MEMORY.md` |

## O fio que atravessa a sessão inteira

**Seis sinais afirmaram completude que não tinham**, e cada um custou um passo:

1. `--brief` elidia 24 de 30 linhas reportando `truncated: false`.
2. O G1 entregava um programa fundido cortado no meio de um caminho, com as
   aspas equilibradas.
3. `dag_updated: true` sobre um subtask inexistente.
4. `updated: true` ao lado de `subtask_updated: false` na mesma resposta.
5. Três testes verdes sobre um perfil que ninguém consultava (`Run("rg")` nunca
   era pedido; o pedido era `Run(any)`).
6. Um guard anti-placeholder que só procurava `<`, enquanto o placeholder era
   prosa num comentário.

É a família de [[sinais-de-progresso-que-mentem]] e de
[[teste-do-componente-nao-e-teste-do-caminho]]. A regra que sobrou dela:
**afirmar a invariante na forma positiva e sobre todos os casos** — verificar a
ausência de UMA forma conhecida do defeito é o que faz o defeito voltar na
próxima forma.

## Fontes primárias (clone local, ler antes de redesenhar)

`/home/gabrielgadea/references/code-mode-2026-08-23/deepseek-harness`

- `.agents/notes/implemented/feature/2026-06-15-code-mode.md` — transporte `run_code`, seam do runtime, `deferContext`, contrato `{code, description}`
- `.agents/notes/implemented/bug-fix/2026-08-07-code-mode-executor-collapse.md` — *"schema omission is not enforcement… denial must be tested through the executor"*; a negação carrega a rota
- `.agents/notes/implemented/feature/2026-08-05-per-agent-tool-presentation.md` — `presentAs` por escopo
- `packages/core/agent-tool-presentation/README.md` — efeito no KV cache

Duas recusas deles que **adotamos**: code mode obrigatório (*"taxes the common
case"*) e reivindicação incondicional de economia. E um desenho que eles
**adiaram** por falta de evidência — níveis por tool — que é o nosso P2, porque a
evidência que faltava a eles é a nossa medição de 1.231 chamadas Bash.
