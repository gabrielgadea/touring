---
type: Resume
title: "Retomar aqui — /tmp × sandbox × portfólio (02/09/2026)"
description: "Estado da execução B+C (decisão de Gabriel) e da wave W (wiring): o que está entregue, o que falta provar ao vivo, e o próximo comando."
tags: [code-mode, ceg, portfolio, wiring, retomar]
timestamp: 2026-09-02T07:45:00-03:00
plan_id: 2026-09-02-tmp-sandbox-portfolio-afordancia
---

# Retomar aqui — 02/09/2026

**Decisão de Gabriel (canvas)**: Canvas 1 → abrir **B + C** (executor CEG + biblioteca), A/A′ não escolhidas. Canvas 2 → **W**, investigar o wiring antes de rejulgar a TIER-2.

**DAG de execução**: `task_1788340910617776382` — W done; B1 B2 B3 C1 C2 C3 C4 **completed** (marcados diretamente após o fechamento; ver /log.md, entrada do resolvedor).

## Entregue (código, sob TDD, verde)

| Fase | O quê | Prova |
|---|---|---|
| W | 4 furos do detector de wiring (free calls no `.scm`, `super`/`self` resolvidos, `./` canonizado + migração, `record_inferred_consumers` no rebuild E no hook) | órfãos escopados 5535→1754 após rebuild com 30.4.31 |
| B1 | tmp PRIVADO por run no CEG (`RunTmp`, `TMPDIR` do run, Landlock troca `/tmp` pelo dir do run), `tmp_bytes` medido, `extra_write_roots` atravessa os rulesets empilhados | touring-ceg 582/582 |
| B2 | `run_journal` v2: source/file/harvest/brief/orchestrate/tmp_bytes + agregados | journal v2_tests, journal_v2_tests, run_source_tests |
| B3 | F2.1 XSS só dentro de tag HTML aberta (kwarg python `onerror=` não é P0) | cwe_patterns tests |
| C1 | escada automática: `snippet:auto:<sig12>`, persiste provisional na 2ª execução limpa | auto_harvest_tests |
| C2 | bloco do scratchpad do harness com exit 0 copiado para `<cwd>/.touring/scratch/<YYYY-MM>/` (`persisted_as`) | scratch_persist_tests |
| C3 | prior-art pré-write deriva intenção da docstring, ignora `#tags:`/shebang | intent_codetag_tests |
| C4 | KPI `code_mode_reuse` (`reuse_ratio`, piso 0.20, STUB/PASS/FAIL) em `touring kpi -j` | reuse_tests |

## Provado ao vivo (30.4.32/33, não por rótulo de versão)

| Prova | Evidência |
|---|---|
| B1 tmp privado | filho vê `TMPDIR=/tmp/touring-run-7xIOVX`; escrita foi para `/tmp/touring-run-hqPfKC/left.bin` |
| B4 `tmp_bytes` no CLI | `"tmp_bytes": 8192` no payload de `touring run` |
| B5 família de escrita | `X6 denied the file-write capability 'write_bytes('` (antes escrevia 8 KiB com `forbidden_calls: []`) |
| B2 journal v2 | `source:"file"`, `file` casando o path |
| C1 escada | `harvest_hint` + `snippet_trust: ○ untrusted` no payload |
| C2 scratch persistente | `persisted_as: .touring/scratch/2026-09/reusable_probe_block.py`, arquivo em disco |
| C4 régua | `code_mode_reuse` = `reuse_ratio 0.133`, `status FAIL` (piso 0.20) |

Fases extras que a prova ao vivo obrigou: **B4** (`tmp_bytes` nunca chegava ao payload do CLI),
**B5** (X6 só casava `.write(`), **C5** (`#origin:` não é faceta canônica — a tag era descartada).

## Próximo comando (ordem)

```bash
# 1. build+deploy com B4/B5/C5 (bump já feito)
setsid nohup update-touring --no-kill --no-restart > <scratch>/update.log 2>&1 < /dev/null &
update-touring --no-build
# 2. índice com o detector W no hook
touring index rebuild --dir /home/gabrielgadea/projects/touring
# 3. rejulgar TIER-2 (DAG já 0 pendentes) e este bundle
python3 ~/.claude/skills/loop-engineering/scripts/loop_converged.py --task task_1788318067626657742 --scope /home/gabrielgadea/projects/touring --bundle docs/plans/2026-08-31-complementacao-hooks
python3 ~/.claude/skills/loop-engineering/scripts/loop_converged.py --task task_1788340910617776382 --scope /home/gabrielgadea/projects/touring --bundle docs/plans/2026-09-02-tmp-sandbox-portfolio-afordancia
# 4. propagar + registrar PostToolUseFailure → touring-hook post-bash (autorizado)
scripts/propagate-release.sh <versão>
```

## Dívida real remanescente (uma decisão para Gabriel)

Contra o baseline **normalizado** (3148, depois de desfazer o prefixo `./` que a W3 removeu), os
órfãos caíram para 1740: 1426 resolvidos pelo detector e 18 realmente novos, todos triados. Oito
eram falsos (homônimos e despacho por tabela de comandos), sete eram `pub` demais e foram
estreitados, e dois (`duration_p50`/`duration_p99`) foram ligados ao KPI de sinal.

Sobra **um**: `sdk::load_signal_report` — API pública documentada, sem consumidor. Integrar exige
inventar o consumidor; remover apaga superfície. Não decidi sozinho.

Achado aberto para uma wave própria: chamada via **tabela de comandos** (ponteiro de função) não é
`call_expression`, então todo handler despachado assim lê como órfão — mesmo formato do furo que
W1 fechou para função livre.

## Memórias

`progresso:tmp-sandbox-portfolio-exec:2026-09-02T07:40` · `w:causa-raiz-orfaos-pos-rebuild:2026-09-02` · `strategy:tmp-sandbox-portfolio-afordancia:2026-09-02`
