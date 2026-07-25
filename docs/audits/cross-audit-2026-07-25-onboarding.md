---
type: CrossAuditReport
title: "Cross-audit — Onboarding per-project (analise/transferegov/konverter) + spawn-sites"
description: "4º audit da série: onboarding com isolamento total nos 3 projetos, fix do 3º spawn-site, e as 2 anomalias abertas ATACADAS — uma virou o finding mais grave da série (cascading daemon kill, F-NEW-4) e foi corrigida e provada; a outra dissolvida com números reais"
tags: [cross-audit, onboarding, per-project, cascading-kill, regra-19]
timestamp: 2026-07-25
plan: /docs/plans/touring-productization-pln2/00-INDEX.md
---

# Cross-Audit 2026-07-25 (4º da série) — Onboarding + Spawn-Sites

## 1 · VERDICT

**PASS — o audit atacou as 2 anomalias que o onboarding deixou abertas e uma
delas era o finding mais grave da série: `update-touring` executava um
CASCADING KILL em todos os daemons per-project a cada deploy (F-NEW-4,
REGRA #19 core). Corrigido e provado ao vivo: os 3 daemons dos projetos
atravessam o ciclo completo kill+restart com os MESMOS PIDs.** A outra
anomalia (contagens idênticas no `index status`) dissolveu-se com evidência:
era a resolução client-side lendo o índice errado por cwd — os índices reais
são distintos e populados (1,29M / 658k / 194k símbolos).

## 2 · SCORECARD

| Eixo | Resultado | Evidência executada |
|---|---|---|
| Onboarding 3 projetos | init+pin+opt-in+registro+CLAUDE.md+índice ✅×3 | provas por projeto |
| Daemons per-project | **4 vivos** (global dev-channel + 3 pinados), **PIDs estáveis pós-fix** | list-all + /proc |
| **Prova F-NEW-4** | ANTES==DEPOIS nos 3 PIDs através de `update-touring` completo | 2ª rodada (kill-step já com binário corrigido) |
| Índices isolados | analise **1.296.556 sym/41.868 files** · transferegov **658.371/4.578** · konverter **194.610/3.935** | `index status` com root correto |
| Suite lib | **1420 passed, 0 failed** | release, pós-fix |
| clippy / daemon_ctl tests | 0 · 17/17 | gate do fix |
| doctor / e2e | 6/6 ok · composite 0.8660 pass | ao vivo |

## 3 · FINDINGS (all-breadth)

**F-NEW-4 · ALTO · CONFIRMADO → CORRIGIDO+PROVADO.** Cascading daemon kill:
`cmd_stop`/`cmd_restart` com dono do socket não-identificado caíam em
`all_daemon_pids()` ("reap them all" — pré-W12.5, quando múltiplos daemons só
podiam ser órfãos split-brain). Na era multi-daemon, esse fallback SIGTERMava
os daemons per-project de TODOS os projetos. Gatilho real observado: o lock
do global com PID stale de um dia (o conteúdo do lock não é reescrito em
respawns) → `pid_for_socket` None → cascata a cada `update-touring` (mortes
"silenciosas" do konverter/transferegov que o onboarding registrou). Fix
duplo: (1) `pid_for_socket` registry-first (a entry W12.5 é reescrita a cada
bind — sempre fresca; lock vira fallback legacy), (2) o reap fallback agora
EXCLUI donos registrados de outros sockets (`orphan_daemon_pids`). Prova:
1ª rodada ainda matou (o kill-step do update-touring roda o binário
INSTALADO=velho — armadilha de bootstrap dos fixes de lifecycle); 2ª rodada
com binário corrigido: **os 3 PIDs intactos**. Commit `0f24831`; toolchain
reinstalada.

**A-RESOLVIDA · `index status` "idêntico" não era bug de isolamento.** O
comando responde do symbol_store do PROCESSO CLIENTE (root resolvido por
CLAUDE_PROJECT_DIR/cwd) — minhas 2 consultas rodaram com cwd no touring
workspace e leram o MESMO índice. Com o root correto, os 3 projetos reportam
contagens reais e distintas. Família client-side agora com 3 membros
(doctor `project_db`, `index status`, + o environ do daemon como fonte de
verdade) — candidata a correção sistêmica: comandos de status deveriam
consultar o DAEMON alvo, não o estado local do cliente (roteado).

**O1.** As "mortes silenciosas de daemons ociosos" do onboarding eram, na
verdade, o F-NEW-4 (cascatas dos meus próprios deploys) — idle watchdog está
OFF por default (verificado no código e nos environs). Com o fix, PIDs
estáveis desde então.

**O2 (fix da rodada anterior, integrado).** 3º spawn-site (autospawn do
hook) ganhou root-pinning derivado do socket — provado: respawn do
transferegov nasceu com `TOURING_PROJECT_ROOT` do projeto (commit `0ac69de`).

## 4 · FUSED RISK

Nenhum P0. Residuais: (a) família client-side (roteamento de status → daemon
alvo) — melhoria sistêmica roteada; (b) armadilha de bootstrap em fixes de
lifecycle: a 1ª execução pós-fix do update-touring usa o binário velho no
kill-step (documentada — rodar update-touring 2× após fixes de daemon-ctl);
(c) Actions GitHub segue sem minutos (decisão de visibilidade/billing).

## 5 · ROOT-CAUSE

Extensão da classe da série: **"código correto para a topologia antiga vira
arma na topologia nova"** — `all_daemon_pids` era seguro no mundo singleton
(múltiplos = órfãos); o multi-daemon o transformou em cascading kill. Par com
os F-NEW-1/2/3 (consumidores esquecidos): aqui o esquecido foi um FALLBACK.
Census de consumidores E de fallbacks ao mudar a cardinalidade de um recurso.

## 6 · PROVENANCE

Investigação: grep idle/env + exit markers (ausentes) + lock global stale
(PID 1764214 morto) + leitura cmd_stop/restart/all_daemon_pids + registry
W12.5 real (`/tmp/touring-daemons-1000/*.json`) · Fix: Edit daemon_ctl
(pid_for_socket registry-first + orphan_daemon_pids) + 2 call-sites ·
Provas: snapshot PIDs antes/depois de update-touring (2 rodadas) + index
status com root correto ×3 + suite 1420 + clippy + doctor + e2e + list-all ·
Commits `0ac69de` (3º spawn-site), `0f24831` (F-NEW-4) pushed.

## 7 · ACTIONS

**Fechadas**: F-NEW-4 (fix+prova E2E), anomalia index-status (dissolvida com
números), 3º spawn-site, onboarding 3 projetos completo. **Roteadas**:
roteamento de comandos de status para o daemon alvo (melhoria sistêmica);
regra operacional "update-touring 2× após fix de lifecycle"; Actions
(decisão de Gabriel). **Estado**: 4 audits na série, 5 findings reais
(F-NEW-1..4 + lsp E0277), todos corrigidos e provados.

---
_4º cross-audit (1º F4′ · 2º F1+F2 · 3º F3→descolamento · 4º onboarding).
Institucional: `touring memory store cross-audit-onboarding-2026-07-25`._
