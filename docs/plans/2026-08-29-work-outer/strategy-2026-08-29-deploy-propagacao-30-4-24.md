---
type: Strategy
title: Estratégia — deploy e propagação do touring 30.4.24
description: Rota canônica do release 30.4.24 (replay offline OnlineRL + retenção de identidade + KPIs de política) a todos os projetos consumidores, com prova comportamental no gate 5.5.
plan_id: 2026-08-29-work-outer
tags: [strategy, release, deploy, propagation]
timestamp: 2026-08-29T22:40:00-03:00
okf_version: "0.1"
---

# Estratégia — deploy e propagação 30.4.24

Ordem de Gabriel: "faça o deploy e atualize o touring em todos os projetos".

## Rota (canônica, não inventada)

`scripts/propagate-release.sh 30.4.24` — pipeline completo da regra 2.1 do
CLAUDE.md do workspace: gates (cargo check + clippy -D warnings + espelho
client/) → `update-touring` (L1→dev) → `toolchain install --from-source .
30.4.24 --force` (L2, snapshot imutável) → `toolchain default` → `touring
update 30.4.24 --project <p>` projeto a projeto (L4; NUNCA `--all-projects`
com canal — arrastaria a fonte) → verify por projeto + gate 5.5 (prova
comportamental, 35 asserções, retry-once).

## Decisões desta janela

1. **Bump 30.4.23 → 30.4.24 antes de propagar** (commit `8bb6faa`): o rótulo
   30.4.23 já nomeava o binário anterior aos commits de RL (`306035c`,
   `611231d`); propagar o mesmo rótulo com conteúdo novo recriaria o gotcha
   "rótulo não prova build". A prova final segue comportamental (gate 5.5),
   nunca pelo rótulo.
2. **Working tree**: código 100% commitado antes do build; só KPIs/logs
   gerados e `.profraw` de coverage ficam fora (não afetam o binário).
3. **Execução em background** com retomada por flags `--skip-*` se o teto de
   10 min do harness cortar o pipeline no meio (build release é a etapa longa).
4. **Peer `analise-e0` avisada** do restart iminente do daemon per-project
   dela durante o `touring update` do root `~/projects/analise`.

## Convergência

O veredito do deploy é o exit 0 do `propagate-release.sh` + a tabela de
verify por projeto (versão resolvida do lock + prova comportamental), nunca
a narrativa. Ledger CCE do topic convergido (external waived: deploy interno,
rota documentada; sem biblioteca externa a consultar).
